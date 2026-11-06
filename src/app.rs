use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use image::DynamicImage;
use rand::seq::SliceRandom;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::audio::Player;
use crate::cli::{Args, GraphicsArg, ModeArg};
use crate::graphics::{Graphics, Protocol};
use crate::i18n;
use crate::model::{Library, Track};
use crate::scan::{spawn_scan, ScanMsg};
use crate::spectrum::Analyzer;
use crate::term::Caps;
use crate::theme::Theme;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Seq,
    Rev,
    Shuffle,
    One,
}

impl Mode {
    pub fn label(&self) -> &'static str {
        match self {
            Mode::Seq => "SEQ",
            Mode::Rev => "REV",
            Mode::Shuffle => "SHUF",
            Mode::One => "ONE",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Mode::Seq => "顺序播放",
            Mode::Rev => "倒序播放",
            Mode::Shuffle => "随机播放",
            Mode::One => "单曲循环",
        }
    }

    pub fn next(&self) -> Mode {
        match self {
            Mode::Seq => Mode::Rev,
            Mode::Rev => Mode::Shuffle,
            Mode::Shuffle => Mode::One,
            Mode::One => Mode::Seq,
        }
    }

    pub fn from_arg(a: ModeArg) -> Mode {
        match a {
            ModeArg::Seq => Mode::Seq,
            ModeArg::Rev => Mode::Rev,
            ModeArg::Shuffle => Mode::Shuffle,
            ModeArg::One => Mode::One,
        }
    }
}

pub struct App {
    pub root: PathBuf,
    pub lib: Option<Library>,
    scan_rx: Receiver<ScanMsg>,
    pub scan_seen: usize,
    pub scan_file: String,
    pub failed: Option<String>,

    pub view: Vec<usize>,
    pub sel: usize,
    pub offset: usize,
    pub query: String,
    pub searching: bool,

    pub base: Vec<usize>,
    pub order: Vec<usize>,
    pub qpos: usize,
    pub mode: Mode,

    pub now: Option<usize>,
    pub player: Option<Player>,
    pub pos: Duration,
    pub dur: Duration,
    pub volume: u8,

    pub err: Option<String>,
    pub toast: Option<(String, Instant)>,
    pub help: bool,
    pub quit: bool,
    pub tick: u64,

    pub cover_rect: Option<Rect>,
    pub cover_key: u64,
    pub cover_img: Option<Arc<DynamicImage>>,
    cover_cache: HashMap<usize, Option<Arc<DynamicImage>>>,

    pub spectrum: Vec<f32>,
    analyzer: Analyzer,
    samples: Vec<f32>,

    pub graphics: Graphics,
    pub theme: Theme,
    pub caps: Caps,
    pub aspect: f32,
    pub auto_play: bool,
    initial_file: Option<PathBuf>,
}

impl App {
    pub fn new(args: &Args, caps: Caps) -> App {
        let initial_file = if args.path.is_file() {
            Some(args.path.clone())
        } else {
            None
        };
        let root = if args.path.is_file() {
            args.path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        } else {
            args.path.clone()
        };
        let root = std::fs::canonicalize(&root).unwrap_or(root);

        let proto = match args.graphics {
            GraphicsArg::Auto => {
                if caps.kitty {
                    Protocol::Kitty
                } else if caps.iterm {
                    Protocol::Iterm2
                } else {
                    Protocol::Blocks
                }
            }
            GraphicsArg::Kitty => Protocol::Kitty,
            GraphicsArg::Iterm => Protocol::Iterm2,
            GraphicsArg::Blocks => Protocol::Blocks,
            GraphicsArg::Off => Protocol::Off,
        };

        let aspect = args.cell_aspect.unwrap_or_else(|| caps.aspect());
        let aspect = if (1.2..=4.0).contains(&aspect) {
            aspect
        } else {
            2.1
        };

        let mut player = if args.render {
            None
        } else {
            Some(Player::spawn(args.volume as f32 / 100.0))
        };
        if args.volume == 0 {
            if let Some(p) = &player {
                p.set_volume(0.0);
            }
        }
        if let Some(p) = &player {
            if !p.alive() {
                player = None;
            }
        }

        let scan_rx = spawn_scan(root.clone(), !args.no_recursive);

        let kitty_ok = caps.kitty;
        let mut app = App {
            root,
            lib: None,
            scan_rx,
            scan_seen: 0,
            scan_file: String::new(),
            failed: None,
            view: Vec::new(),
            sel: 0,
            offset: 0,
            query: String::new(),
            searching: false,
            base: Vec::new(),
            order: Vec::new(),
            qpos: 0,
            mode: Mode::from_arg(args.mode),
            now: None,
            player,
            pos: Duration::ZERO,
            dur: Duration::ZERO,
            volume: args.volume,
            err: None,
            toast: None,
            help: false,
            quit: false,
            tick: 0,
            cover_rect: None,
            cover_key: 0,
            cover_img: None,
            cover_cache: HashMap::new(),
            spectrum: vec![0.0; crate::spectrum::BANDS],
            analyzer: Analyzer::new(44_100.0),
            samples: Vec::new(),
            graphics: Graphics::new(proto, caps.cell_w, caps.cell_h),
            theme: Theme::from_light(caps.light),
            caps,
            aspect,
            auto_play: args.play,
            initial_file,
        };

        // Say why the cover is not a picture, instead of just looking broken.
        if !args.render && !args.probe {
            match (args.graphics, proto) {
                (GraphicsArg::Auto, Protocol::Blocks) => app.toast(i18n::t(
                    "终端没有图形协议，封面改用半块字符（ymp --probe 查看详情）",
                    "no graphics protocol here, covers use half blocks (ymp --probe for details)",
                )),
                (GraphicsArg::Kitty, _) if !kitty_ok => app.toast(i18n::t(
                    "终端没有回应 Kitty 图形协议，封面可能不显示（ymp --probe）",
                    "the terminal did not answer the kitty probe, covers may stay blank (ymp --probe)",
                )),
                _ => {}
            }
        }
        app
    }

    pub fn tracks(&self) -> &[Track] {
        self.lib.as_ref().map(|l| l.tracks.as_slice()).unwrap_or(&[])
    }

    pub fn track(&self, idx: usize) -> Option<&Track> {
        self.tracks().get(idx)
    }

    pub fn focus_index(&self) -> Option<usize> {
        self.now.or_else(|| self.view.get(self.sel).copied())
    }

    pub fn cover_for(&mut self, idx: usize) -> Option<Arc<DynamicImage>> {
        if let Some(cached) = self.cover_cache.get(&idx) {
            return cached.clone();
        }
        let bytes = self
            .lib
            .as_ref()
            .and_then(|l| l.tracks.get(idx))
            .and_then(|t| t.cover.clone());
        let img = bytes.and_then(|b| image::load_from_memory(&b).ok()).map(Arc::new);
        if self.cover_cache.len() > 48 {
            self.cover_cache.clear();
        }
        self.cover_cache.insert(idx, img.clone());
        img
    }

    fn rebuild_view(&mut self) {
        let q = self.query.trim().to_lowercase();
        let prev_track = self.view.get(self.sel).copied();
        self.view = self
            .tracks()
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                if q.is_empty() {
                    return true;
                }
                t.title.to_lowercase().contains(&q)
                    || t.artist.to_lowercase().contains(&q)
                    || t.album.to_lowercase().contains(&q)
                    || t.path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();
        self.sel = prev_track
            .and_then(|t| self.view.iter().position(|&i| i == t))
            .unwrap_or(0)
            .min(self.view.len().saturating_sub(1));
        self.offset = 0;
    }

    fn rebuild_order(&mut self, start: Option<usize>) {
        let mut order = self.base.clone();
        match self.mode {
            Mode::Seq | Mode::One => {}
            Mode::Rev => order.reverse(),
            Mode::Shuffle => {
                if let Some(cur) = start {
                    order.retain(|&i| i != cur);
                }
                let mut rng = rand::rng();
                order.shuffle(&mut rng);
                if let Some(cur) = start {
                    order.insert(0, cur);
                }
            }
        }
        if let Some(cur) = start {
            if let Some(pos) = order.iter().position(|&i| i == cur) {
                order.rotate_left(pos);
            }
        }
        self.order = order;
        self.qpos = 0;
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.rebuild_order(self.now);
        self.toast(format!("{} · {}", mode.label(), mode.hint()));
    }

    fn start_playback(&mut self, idx: usize, reset_base: bool) {
        if reset_base {
            self.base = self.view.clone();
        }
        if self.base.is_empty() {
            self.base = (0..self.tracks().len()).collect();
        }
        self.rebuild_order(Some(idx));
        self.qpos = self.order.iter().position(|&i| i == idx).unwrap_or(0);
        self.play_index(idx);
    }

    fn play_index(&mut self, idx: usize) {
        let Some(track) = self.track(idx) else { return };
        let path = track.path.clone();
        let duration = track.duration;
        self.now = Some(idx);
        self.pos = Duration::ZERO;
        self.dur = duration;
        if let Some(pos) = self.view.iter().position(|&i| i == idx) {
            self.sel = pos;
        }
        self.cover_cache.shrink_to(24);
        if let Some(p) = &self.player {
            p.set_volume(self.volume as f32 / 100.0);
            p.load(&path, 0.0);
        }
    }

    fn advance(&mut self, auto: bool) {
        match self.mode {
            Mode::One if auto => {
                if let Some(idx) = self.now {
                    self.play_index(idx);
                }
            }
            _ => {
                if self.qpos + 1 < self.order.len() {
                    self.qpos += 1;
                    let idx = self.order[self.qpos];
                    self.play_index(idx);
                } else {
                    self.now = None;
                    self.pos = Duration::ZERO;
                    if let Some(p) = &self.player {
                        p.stop();
                    }
                    self.toast(i18n::t("播放列表结束", "end of the playlist").to_string());
                }
            }
        }
    }

    fn step_back(&mut self) {
        if self.qpos > 0 {
            self.qpos -= 1;
            let idx = self.order[self.qpos];
            self.play_index(idx);
        } else if let Some(idx) = self.now {
            self.play_index(idx);
        }
    }

    fn toggle(&mut self) {
        if let Some(p) = &self.player {
            if self.now.is_none() {
                if let Some(idx) = self.view.get(self.sel).copied() {
                    self.start_playback(idx, true);
                }
                return;
            }
            p.toggle();
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn seek_rel(&mut self, delta: f64) {
        let cur = self.pos.as_secs_f64() + delta;
        let max = self.dur.as_secs_f64();
        let target = cur.clamp(0.0, if max > 0.0 { max - 0.2 } else { 0.0 });
        if let Some(p) = &self.player {
            p.seek(target);
            self.pos = Duration::from_secs_f64(target);
        }
    }

    fn change_volume(&mut self, delta: i16) {
        let v = (self.volume as i16 + delta).clamp(0, 100) as u8;
        self.volume = v;
        if let Some(p) = &self.player {
            p.set_volume(v as f32 / 100.0);
        }
        self.toast(i18n::p(format!("音量 {v}%"), format!("volume {v}%")));
    }

    fn move_sel(&mut self, delta: i32, page: i32) {
        if self.view.is_empty() {
            return;
        }
        let len = self.view.len() as i32;
        let mut s = self.sel as i32 + delta * page;
        if s < 0 {
            s = 0;
        }
        if s >= len {
            s = len - 1;
        }
        self.sel = s as usize;
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if self.help {
            // Nothing to restore: the cover was never taken down.
            self.help = false;
            return;
        }

        if self.searching {
            match key.code {
                KeyCode::Esc => {
                    self.searching = false;
                    self.query.clear();
                    self.rebuild_view();
                }
                KeyCode::Enter => {
                    self.searching = false;
                    if let Some(idx) = self.view.get(self.sel).copied() {
                        self.start_playback(idx, true);
                    }
                }
                KeyCode::Backspace => {
                    self.query.pop();
                    self.rebuild_view();
                }
                KeyCode::Up => self.move_sel(-1, 1),
                KeyCode::Down => self.move_sel(1, 1),
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.push(c);
                    self.rebuild_view();
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.rebuild_view();
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.quit = true;
            }
            KeyCode::Char('?') | KeyCode::Char('h') => {
                // The help panel is drawn inside the list column, so the cover
                // stays on screen; kitty would paint it over the text otherwise.
                self.help = true;
            }
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Enter | KeyCode::Char('l') => {
                if let Some(idx) = self.view.get(self.sel).copied() {
                    self.start_playback(idx, true);
                }
            }
            KeyCode::Char('n') | KeyCode::Tab => self.advance(false),
            KeyCode::Char('p') | KeyCode::BackTab => self.step_back(),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1, 1),
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1, 1),
            KeyCode::PageDown | KeyCode::Char('d') => self.move_sel(1, 8),
            KeyCode::PageUp | KeyCode::Char('u') => self.move_sel(-1, 8),
            KeyCode::Char('g') | KeyCode::Home => self.sel = 0,
            KeyCode::Char('G') | KeyCode::End => {
                self.sel = self.view.len().saturating_sub(1);
            }
            KeyCode::Left | KeyCode::Char(',') => self.seek_rel(-5.0),
            KeyCode::Right | KeyCode::Char('.') => self.seek_rel(5.0),
            KeyCode::Char('+') | KeyCode::Char('=') => self.change_volume(5),
            KeyCode::Char('-') | KeyCode::Char('_') => self.change_volume(-5),
            KeyCode::Char('m') => {
                let m = self.mode.next();
                self.set_mode(m);
            }
            KeyCode::Char('1') => self.set_mode(Mode::Seq),
            KeyCode::Char('2') => self.set_mode(Mode::Rev),
            KeyCode::Char('3') => self.set_mode(Mode::Shuffle),
            KeyCode::Char('4') => self.set_mode(Mode::One),
            KeyCode::Char('/') => {
                self.searching = true;
                self.query.clear();
                self.rebuild_view();
            }
            KeyCode::Char('r') => {
                self.cover_cache.clear();
                self.graphics.invalidate();
                self.toast(i18n::t("封面缓存已清空", "cover cache cleared"));
            }
            _ => {}
        }
    }

    pub fn poll_scan(&mut self) {
        self.drain_scan();
    }

    fn drain_scan(&mut self) {
        while let Ok(msg) = self.scan_rx.try_recv() {
            match msg {
                ScanMsg::Progress(n, path) => {
                    self.scan_seen = n;
                    self.scan_file = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                }
                ScanMsg::Done(lib) => {
                    self.base = (0..lib.tracks.len()).collect();
                    self.view = self.base.clone();
                    self.lib = Some(*lib);
                    self.sel = 0;
                    self.rebuild_order(None);

                    if let Some(file) = self.initial_file.clone() {
                        if let Some(pos) = self
                            .tracks()
                            .iter()
                            .position(|t| t.path == file || t.path.ends_with(&file))
                        {
                            self.sel = pos;
                            if self.auto_play {
                                self.start_playback(pos, true);
                            }
                        }
                    } else if self.auto_play {
                        self.start_playback(0, true);
                    }
                }
                ScanMsg::Failed(e) => self.failed = Some(e),
            }
        }
    }

    pub fn tick(&mut self) -> Result<bool> {
        self.drain_scan();

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(k) => self.on_key(k),
                Event::Resize(_, _) => {
                    self.graphics.invalidate();
                    self.cover_rect = None;
                }
                _ => {}
            }
        }

        let mut ended = false;
        if let Some(p) = &self.player {
            if self.now.is_some() {
                self.pos = p.position();
                let d = p.duration();
                if d > Duration::ZERO {
                    self.dur = d;
                }
                ended = p.take_ended();
            }
            if let Some(e) = p.take_error() {
                self.err = Some(e);
            }
        }
        if ended && self.now.is_some() {
            self.advance(true);
        }

        if self.now.is_some() {
            let mut buf: Vec<f32> = Vec::new();
            let mut rate = 44_100.0f32;
            if let Some(p) = &self.player {
                p.tap(&mut buf);
                rate = p.rate() as f32;
            }
            if buf.is_empty() {
                buf = std::mem::take(&mut self.samples);
            }
            self.samples = buf;
            self.spectrum = self.analyzer.update(&self.samples, rate).to_vec();
        }

        if let Some((_, at)) = &self.toast {
            if at.elapsed() > Duration::from_secs(3) {
                self.toast = None;
            }
        }

        self.tick += 1;
        Ok(self.quit)
    }

    pub fn run(&mut self) -> Result<()> {
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        let _ = terminal.hide_cursor();

        let result = self.event_loop(&mut terminal);

        let mut out = std::io::stdout();
        let _ = self.graphics.clear_all(&mut out);
        result
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        loop {
            if self.tick()? {
                break;
            }
            terminal.draw(|f| ui::draw(f, self))?;

            let rect = self.cover_rect;
            let key = self.cover_key;
            let img = self.cover_img.clone();
            if let Some(rect) = rect {
                let mut out = std::io::stdout();
                let _ = self
                    .graphics
                    .present(&mut out, rect, key, img.as_deref());
            }
        }
        if let Some(p) = &self.player {
            p.quit();
        }
        Ok(())
    }
}
