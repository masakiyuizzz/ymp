use std::time::Duration;

use anyhow::Result;

use crate::audio::decoder::AudioDecoder;
use crate::audio::Player;
use crate::cli::Args;
use crate::scan;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::term::Caps;

pub fn run(args: &Args) -> Result<()> {
    let root = if args.path.is_file() {
        args.path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    } else {
        args.path.clone()
    };

    println!("== scan ==");
    let start = std::time::Instant::now();
    let lib = scan::scan(&root, !args.no_recursive, None);
    println!(
        "root       {}",
        root.display()
    );
    println!(
        "tracks     {}   albums {}   total {}",
        lib.tracks.len(),
        lib.album_count,
        crate::ui::fmt_time(lib.total_duration())
    );
    println!("scan time  {:?}", start.elapsed());
    for (i, t) in lib.tracks.iter().take(4).enumerate() {
        println!(
            "  {:02}  {:<40} {:<18} {:<20} {}  cover={}",
            i + 1,
            t.title,
            t.artist,
            t.album,
            crate::ui::fmt_time(t.duration),
            t.cover.as_ref().map(|c| c.len()).unwrap_or(0)
        );
    }

    println!("== decode ==");
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut total_frames = 0u64;
    for t in &lib.tracks {
        match AudioDecoder::open(&t.path) {
            Ok(mut d) => {
                let rate = d.rate;
                let ch = d.channels;
                let mut frames = 0usize;
                let mut chunks = 0;
                loop {
                    match d.next_chunk() {
                        Ok(Some(s)) => {
                            frames += s.len() / 2;
                            chunks += 1;
                            if chunks >= 40 {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            println!("  decode error {}: {e}", t.path.display());
                            failed += 1;
                            break;
                        }
                    }
                }
                total_frames += frames as u64;
                ok += 1;
                if ok == 1 {
                    println!(
                        "  first: {}Hz {}ch  {} frames in {} chunks",
                        rate, ch, frames, chunks
                    );
                }
            }
            Err(e) => {
                println!("  open failed {}: {e}", t.path.display());
                failed += 1;
            }
        }
    }
    println!("  decoded {ok} files, {failed} failed, {total_frames} frames sampled");

    println!("== cover ==");
    let with_cover = lib.tracks.iter().filter(|t| t.cover.is_some()).count();
    println!("  {with_cover}/{} tracks carry artwork", lib.tracks.len());
    if let Some(t) = lib.tracks.iter().find(|t| t.cover.is_some()) {
        let bytes = t.cover.as_ref().unwrap();
        match image::load_from_memory(bytes) {
            Ok(img) => println!(
                "  sample: {}x{} {:?} from {}",
                img.width(),
                img.height(),
                img.color(),
                t.title
            ),
            Err(e) => println!("  sample decode failed: {e}"),
        }
    }

    println!("== audio output ==");
    let Some(first) = lib.tracks.first() else {
        println!("  no tracks, skip");
        return Ok(());
    };
    let player = Player::spawn(0.6);
    std::thread::sleep(Duration::from_millis(300));
    if !player.alive() {
        println!("  audio engine did not start");
        if let Some(e) = player.take_error() {
            println!("  error: {e}");
        }
        return Ok(());
    }
    player.load(&first.path, 0.0);
    std::thread::sleep(Duration::from_millis(1600));
    let p1 = player.position();
    println!(
        "  playing 1.6s -> position {}  duration {}  paused {}",
        fmt_ms(p1),
        fmt_ms(player.duration()),
        player.paused()
    );
    player.toggle();
    std::thread::sleep(Duration::from_millis(400));
    let p2 = player.position();
    std::thread::sleep(Duration::from_millis(600));
    let p3 = player.position();
    println!(
        "  paused -> position {} then {} (frozen: {})",
        fmt_ms(p2),
        fmt_ms(p3),
        p2 == p3
    );
    player.toggle();
    std::thread::sleep(Duration::from_millis(800));
    println!("  resumed -> position {}", fmt_ms(player.position()));

    player.seek(60.0);
    std::thread::sleep(Duration::from_millis(900));
    let p4 = player.position();
    println!(
        "  after seek 60s -> position {} (ok: {})",
        fmt_ms(p4),
        p4 >= Duration::from_secs(60) && p4 < Duration::from_secs(65)
    );

    if let Some(e) = player.take_error() {
        println!("  engine error: {e}");
    }
    player.quit();
    Ok(())
}

pub fn render(args: &Args, width: u16, height: u16) -> Result<()> {
    let mut caps = Caps::default();
    caps.kitty = matches!(args.graphics, crate::cli::GraphicsArg::Kitty);
    caps.iterm = matches!(args.graphics, crate::cli::GraphicsArg::Iterm);
    caps.light = false;
    caps.cell_w = 8;
    caps.cell_h = 17;

    let mut app = App::new(args, caps);
    for _ in 0..200 {
        app.poll_scan();
        if app.lib.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if app.lib.is_none() {
        println!("no library loaded; nothing to render");
        return Ok(());
    }

    app.now = Some(2);
    app.sel = 3;
    app.pos = Duration::from_secs(73);
    app.spectrum = vec![
        0.92, 0.78, 0.61, 0.55, 0.72, 0.88, 0.66, 0.44, 0.58, 0.35, 0.47, 0.29, 0.33, 0.21, 0.14,
        0.08,
    ];
    if let Some(t) = app.track(2) {
        app.dur = t.duration;
    }

    if std::env::var_os("YMP_RENDER_HELP").is_some() {
        app.help = true;
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    let mut app = app;
    terminal.draw(|f| crate::ui::draw(f, &mut app))?;

    let buf = terminal.backend().buffer();
    println!("+{}+", "-".repeat(width as usize));
    for y in 0..height {
        let mut line = String::new();
        for x in 0..width {
            line.push_str(buf[(x, y)].symbol());
        }
        println!("|{}|", line.trim_end());
    }
    println!("+{}+", "-".repeat(width as usize));
    // overflow check: every row must start and end with the frame
    let mut bad = Vec::new();
    for y in 0..height {
        let first = buf[(0, y)].symbol().to_string();
        let last = buf[(width - 1, y)].symbol().to_string();
        let ok_first = matches!(first.as_str(), "│" | "╭" | "╰" | " ");
        let ok_last = matches!(last.as_str(), "│" | "╮" | "╯" | " ");
        if !ok_first || !ok_last {
            bad.push(format!("row {y}: {first:?}..{last:?}"));
        }
    }
    if bad.is_empty() {
        println!("frame check: ok (all {height} rows framed)");
    } else {
        println!("frame check: BROKEN -> {}", bad.join(", "));
    }

    println!(
        "cover rect: {:?}   proto: {}",
        app.cover_rect,
        app.graphics.proto.label()
    );

    if let (Some(rect), Some(img)) = (app.cover_rect, app.cover_img.clone()) {
        let mut sink: Vec<u8> = Vec::new();
        app.graphics.present(&mut sink, rect, 7, Some(&img))?;
        let head_len = sink.len().min(160);
        println!("kitty bytes: {}", sink.len());
        println!("head: {:?}", String::from_utf8_lossy(&sink[..head_len]));
        println!("tail: {:?}", String::from_utf8_lossy(&sink[sink.len().saturating_sub(40)..]));
    }
    Ok(())
}

fn fmt_ms(d: Duration) -> String {
    format!("{:02}:{:05.2}", d.as_secs() / 60, (d.as_secs() % 60) as f64 + d.subsec_millis() as f64 / 1000.0)
}
