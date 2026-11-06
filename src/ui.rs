use std::time::Duration;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::graphics::{placeholder, Protocol};
use crate::i18n;

fn w(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

pub fn fmt_time(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn fit(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if w(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > max.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    out
}

fn short_path(path: &std::path::Path) -> String {
    let s = path.display().to_string();
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy().to_string();
        if let Some(rest) = s.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    s
}

const SIDE_TEXT_H: u16 = 9;
const SPECTRUM_H: u16 = 3;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let t = app.theme.clone();
    f.render_widget(
        Block::default().style(Style::default().fg(Color::Reset).bg(Color::Reset)),
        area,
    );

    app.cover_rect = None;
    app.cover_img = None;

    if area.width < 30 || area.height < 7 {
        f.render_widget(
            Paragraph::new("terminal too small")
                .style(Style::default().fg(t.muted))
                .alignment(Alignment::Center),
            area,
        );
        return;
    }

    // The whole player lives inside one rounded frame that stretches with the
    // window; the title is woven into the top rule so it reads as one line.
    let frame = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .title(top_line(app, area));
    let inner = frame.inner(area);
    f.render_widget(frame, area);

    if inner.width < 24 || inner.height < 4 {
        return;
    }

    let content = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };

    let rows = Layout::vertical([
        Constraint::Min(2),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(content);

    // Where the help panel may be drawn: the list column. Kitty paints pictures
    // on top of text, so a dialog over the cover would be unreadable — keeping
    // it out of the cover's column means the artwork never has to be hidden.
    let mut help_area = rows[0];

    if app.lib.is_none() {
        draw_loading(f, app, rows[0]);
    } else {
        let body = rows[0];
        let show_cover =
            app.graphics.proto != Protocol::Off && body.width >= 58 && body.height >= 12;
        if show_cover {
            let (cols, crou) = cover_size(app, body);
            let split =
                Layout::horizontal([Constraint::Min(22), Constraint::Length(cols + 5)]).split(body);
            draw_vrule(f, app, split[0], body);
            draw_list(f, app, split[0]);
            draw_side(f, app, split[1], cols, crou);
            help_area = split[0];
        } else {
            draw_list(f, app, body);
        }
    }

    draw_progress(f, app, rows[1]);
    draw_hints(f, app, rows[2]);

    if app.help {
        draw_help(f, app, help_area);
    }
}

/// Top rule with the brand, the folder and the library stats woven into it.
fn top_line(app: &App, area: Rect) -> Line<'static> {
    let t = &app.theme;
    let inner_w = area.width.saturating_sub(2) as usize;

    let stats = match &app.lib {
        Some(lib) => format!(
            "{} {} · {} {} · {}",
            lib.tracks.len(),
            if lib.tracks.len() == 1 {
                "track"
            } else {
                "tracks"
            },
            lib.album_count,
            if lib.album_count == 1 {
                "album"
            } else {
                "albums"
            },
            fmt_time(lib.total_duration())
        ),
        None => format!("{} · {}", i18n::t("扫描中", "scanning"), app.scan_seen),
    };

    let border = Style::default().fg(t.border);
    let brand = Span::styled(
        " ymp ",
        Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
    );
    let lead = Span::styled("─", border);
    let tail = Span::styled("─", border);

    // ─ + " ymp " = 6 cells of fixed chrome.
    let stats_w = w(&stats);
    let path = short_path(&app.root);

    // 1 + 5 + (" " path " ") + fill + (" " stats " ") + 1
    let full_need = 1 + 5 + 2 + w(&path) + 1 + 2 + stats_w + 1;
    if inner_w >= full_need && inner_w >= 50 {
        let fill = inner_w - (1 + 5 + 2 + w(&path) + 2 + stats_w + 1);
        return Line::from(vec![
            lead,
            brand,
            Span::styled(format!(" {} ", path), Style::default().fg(t.muted)),
            Span::styled("─".repeat(fill), border),
            Span::styled(format!(" {} ", stats), Style::default().fg(t.dim)),
            tail,
        ]);
    }

    let stats_need = 1 + 5 + 1 + 2 + stats_w + 1;
    if inner_w >= stats_need && inner_w >= 50 {
        let fill = inner_w - (1 + 5 + 2 + stats_w + 1);
        return Line::from(vec![
            lead,
            brand,
            Span::styled("─".repeat(fill), border),
            Span::styled(format!(" {} ", stats), Style::default().fg(t.dim)),
            tail,
        ]);
    }

    // Narrow terminal: keep the brand and just rule the rest.
    let fill = inner_w.saturating_sub(7);
    Line::from(vec![
        lead,
        brand,
        Span::styled("─".repeat(fill), border),
        tail,
    ])
}

fn cover_size(app: &App, body: Rect) -> (u16, u16) {
    let max_rows = body.height.saturating_sub(SIDE_TEXT_H).max(6);
    let by_h = (max_rows as f32 * app.aspect) as u16;
    let by_w = (body.width as f32 * 0.40) as u16;
    let cols = by_h.min(by_w).clamp(10, 56);
    let rows = ((cols as f32) / app.aspect).round() as u16;
    let rows = rows.clamp(4, max_rows);
    (cols, rows)
}

fn draw_vrule(f: &mut Frame, app: &App, list: Rect, body: Rect) {
    let t = &app.theme;
    let x = body.x + list.width;
    if x >= body.x + body.width {
        return;
    }
    let mut lines = Vec::with_capacity(body.height as usize);
    for _ in 0..body.height {
        lines.push(Line::from(Span::styled("│", Style::default().fg(t.faint))));
    }
    f.render_widget(
        Paragraph::new(lines),
        Rect {
            x,
            y: body.y,
            width: 1,
            height: body.height,
        },
    );
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme.clone();
    let h = area.height as usize;
    let len = app.view.len();
    if len == 0 {
        let msg = if app.query.is_empty() {
            "no tracks"
        } else {
            "no match"
        };
        f.render_widget(Paragraph::new(msg).style(Style::default().fg(t.dim)), area);
        return;
    }
    if app.sel >= len {
        app.sel = len - 1;
    }
    if app.sel < app.offset {
        app.offset = app.sel;
    }
    if app.sel >= app.offset + h {
        app.offset = app.sel + 1 - h;
    }
    if app.offset + h > len {
        app.offset = len.saturating_sub(h);
    }

    let title_w = (area.width as usize).saturating_sub(10);
    let mut lines: Vec<Line> = Vec::with_capacity(h);

    for row in 0..h {
        let vi = app.offset + row;
        if vi >= len {
            lines.push(Line::from(""));
            continue;
        }
        let idx = app.view[vi];
        let Some(track) = app.track(idx) else {
            lines.push(Line::from(""));
            continue;
        };
        let selected = vi == app.sel;
        let playing = app.now == Some(idx);

        let num = if playing {
            if app.player.as_ref().map(|p| p.paused()).unwrap_or(true) {
                "‖".to_string()
            } else {
                "▶".to_string()
            }
        } else {
            format!("{:>2}", vi + 1)
        };

        let mut row_style = Style::default();
        if selected {
            row_style = row_style.bg(t.sel_bg);
        }

        let title_style = if playing {
            Style::default()
                .fg(t.accent)
                .add_modifier(Modifier::BOLD)
                .patch(row_style)
        } else if selected {
            Style::default()
                .fg(t.text)
                .add_modifier(Modifier::BOLD)
                .patch(row_style)
        } else {
            Style::default().fg(t.text).patch(row_style)
        };

        let num_style = if playing {
            Style::default().fg(t.accent).patch(row_style)
        } else {
            Style::default().fg(t.dim).patch(row_style)
        };

        let title = fit(&track.title, title_w);
        // always keep at least one blank before the duration
        let pad_n = title_w.saturating_sub(w(&title)) + 1;
        let dur = fmt_time(track.duration);

        let mut spans = vec![
            Span::styled(format!("{num:>2} "), num_style),
            Span::styled(title, title_style),
            Span::styled(" ".repeat(pad_n), row_style),
            Span::styled(
                format!("{dur:>5} "),
                Style::default().fg(t.dim).patch(row_style),
            ),
        ];
        let used: usize = spans.iter().map(|s| w(&s.content)).sum();
        if used < area.width as usize {
            spans.push(Span::styled(
                " ".repeat(area.width as usize - used),
                row_style,
            ));
        }
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn draw_side(f: &mut Frame, app: &mut App, area: Rect, cols: u16, rows: u16) {
    let t = app.theme.clone();
    let Some(idx) = app.focus_index() else { return };
    let Some(track) = app.track(idx).cloned() else {
        return;
    };

    let x = area.x + 2 + (area.width.saturating_sub(4).saturating_sub(cols)) / 2;
    let cover = Rect {
        x,
        y: area.y,
        width: cols,
        height: rows,
    };

    let img = app.cover_for(idx);
    match app.graphics.proto {
        Protocol::Blocks => {
            let lines = match &img {
                Some(img) => app.graphics.blocks(img, cols, rows, false),
                None => placeholder(cols, rows, t.accent, t.dim),
            };
            f.render_widget(Paragraph::new(lines), cover);
        }
        Protocol::Off => {
            let lines = placeholder(cols, rows, t.accent, t.dim);
            f.render_widget(Paragraph::new(lines), cover);
        }
        _ => {
            if let Some(img) = img {
                app.cover_rect = Some(cover);
                app.cover_key = idx as u64;
                app.cover_img = Some(img);
            } else {
                let lines = placeholder(cols, rows, t.accent, t.dim);
                f.render_widget(Paragraph::new(lines), cover);
            }
        }
    }

    let info_y = cover.y + rows + 1;
    let bottom = area.y + area.height;
    if info_y + SPECTRUM_H >= bottom {
        return;
    }

    let spec_rect = Rect {
        x: area.x + 2,
        y: info_y,
        width: cols,
        height: SPECTRUM_H,
    };
    let paused = app.player.as_ref().map(|p| p.paused()).unwrap_or(true);
    let spec = app.spectrum.clone();
    let (low, high) = if paused {
        (t.faint, t.dim)
    } else {
        (t.accent2, t.accent)
    };
    f.render_widget(
        Paragraph::new(spectrum_lines(&spec, cols, SPECTRUM_H, low, high)),
        spec_rect,
    );

    let info_rect = Rect {
        x: area.x + 2,
        y: info_y + SPECTRUM_H + 1,
        width: area.width.saturating_sub(4),
        height: bottom.saturating_sub(info_y + SPECTRUM_H + 1),
    };
    if info_rect.height == 0 {
        return;
    }

    let inner = info_rect.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    for (i, part) in split_title(&track.title, inner, 2).iter().enumerate() {
        let style = if i == 0 {
            Style::default().fg(t.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.text)
        };
        lines.push(Line::from(Span::styled(part.clone(), style)));
    }

    lines.push(Line::from(Span::styled(
        fit(&track.artist, inner),
        Style::default().fg(t.accent2),
    )));

    let album_line = {
        let mut s = track.album.clone();
        if let Some(y) = track.year {
            s.push_str(&format!("  ·  {y}"));
        }
        s
    };
    lines.push(Line::from(Span::styled(
        fit(&album_line, inner),
        Style::default().fg(t.muted),
    )));

    let counter = {
        let pos = app
            .now
            .and_then(|n| app.view.iter().position(|&i| i == n))
            .map(|p| p + 1)
            .unwrap_or_else(|| app.sel + 1);
        format!("{} / {}   {}", pos, app.view.len(), fmt_time(track.duration))
    };
    lines.push(Line::from(Span::styled(
        counter,
        Style::default().fg(t.dim),
    )));

    f.render_widget(Paragraph::new(lines), info_rect);
}

fn spectrum_lines(
    spec: &[f32],
    cols: u16,
    rows: u16,
    low: Color,
    high: Color,
) -> Vec<Line<'static>> {
    let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let bands = spec.len().min(cols as usize).max(1);
    let band_w = ((cols as usize) / bands).max(1);
    let mut out = Vec::with_capacity(rows as usize);

    for r in 0..rows {
        let mut spans: Vec<Span> = Vec::with_capacity(cols as usize);
        for b in 0..bands {
            let v = spec[b].clamp(0.0, 1.0);
            let height = v * rows as f32;
            let cell_bottom = (rows - r) as f32;
            let cell_top = cell_bottom - 1.0;
            let ch = if height >= cell_bottom {
                '█'
            } else if height > cell_top {
                let frac = height - cell_top;
                let idx = ((frac * 8.0).ceil() as usize).clamp(1, 8) - 1;
                blocks[idx]
            } else {
                ' '
            };
            let color = if ch == ' ' {
                Color::Reset
            } else if r == 0 {
                high
            } else {
                low
            };
            let s: String = std::iter::repeat(ch).take(band_w).collect();
            spans.push(Span::styled(s, Style::default().fg(color)));
        }
        let used = spans.len() * band_w;
        if used < cols as usize {
            spans.push(Span::raw(" ".repeat(cols as usize - used)));
        }
        out.push(Line::from(spans));
    }
    out
}

fn split_title(s: &str, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }
    if w(s) <= width {
        return vec![s.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > width {
            out.push(std::mem::take(&mut cur));
            used = 0;
            if out.len() + 1 >= max_lines {
                break;
            }
        }
        cur.push(ch);
        used += cw;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.len() >= max_lines {
        if let Some(last) = out.last_mut() {
            *last = fit(last, width);
        }
    }
    out
}

fn draw_progress(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let pos = fmt_time(app.pos);
    let dur = fmt_time(app.dur);
    let paused = app.player.as_ref().map(|p| p.paused()).unwrap_or(true);

    let mode = app.mode.label();
    let right = format!("{}  vol {:>3}% ", mode, app.volume);
    let fixed = 7 + 1 + w(&dur) + w(&right);
    let bar_w = (area.width as usize).saturating_sub(fixed + 1).max(6);

    let ratio = if app.dur.as_secs_f32() > 0.0 {
        (app.pos.as_secs_f32() / app.dur.as_secs_f32()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = ((bar_w as f32 - 1.0) * ratio).round() as usize;
    let filled = filled.min(bar_w.saturating_sub(1));
    let rest = bar_w.saturating_sub(filled + 1);

    let played = if paused { t.muted } else { t.accent };
    let mut spans = vec![
        Span::styled(format!(" {pos:>5} "), Style::default().fg(t.dim)),
        Span::styled("━".repeat(filled), Style::default().fg(played)),
        Span::styled(
            if app.dur.as_secs() > 0 { "●" } else { " " },
            Style::default().fg(played),
        ),
        Span::styled("─".repeat(rest), Style::default().fg(t.faint)),
        Span::styled(format!("{:>5} ", dur), Style::default().fg(t.dim)),
    ];

    let used: usize = spans.iter().map(|s| w(&s.content)).sum();
    let gap = (area.width as usize).saturating_sub(used + w(&right));
    spans.push(Span::raw(" ".repeat(gap)));
    spans.push(Span::styled(
        mode,
        Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!("  vol {:>3}% ", app.volume),
        Style::default().fg(t.dim),
    ));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

const HINTS_CN: &[(&str, &str)] = &[
    ("space", "播放/暂停"),
    ("n/p", "切歌"),
    ("←/→", "快进"),
    ("m", "模式"),
    ("/", "搜索"),
    ("?", "帮助"),
    ("q", "退出"),
];

const HINTS_EN: &[(&str, &str)] = &[
    ("space", "play/pause"),
    ("n/p", "next/prev"),
    ("←/→", "seek"),
    ("m", "mode"),
    ("/", "search"),
    ("?", "keys"),
    ("q", "quit"),
];

fn hints() -> &'static [(&'static str, &'static str)] {
    if i18n::is_en() {
        HINTS_EN
    } else {
        HINTS_CN
    }
}

fn hints_line(app: &App, width: usize) -> Line<'static> {
    let t = &app.theme;
    let mut spans: Vec<Span> = Vec::new();
    let mut used = 0usize;
    let mut first = true;
    for (k, d) in hints() {
        let seg = if first {
            w(k) + 1 + w(d)
        } else {
            3 + w(k) + 1 + w(d)
        };
        if used + seg > width {
            break;
        }
        if !first {
            spans.push(Span::raw("   "));
        }
        first = false;
        spans.push(Span::styled(
            *k,
            Style::default().fg(t.muted).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*d, Style::default().fg(t.dim)));
        used += seg;
    }
    Line::from(spans)
}

fn draw_hints(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let width = area.width as usize;
    let line = if app.searching {
        let mut spans = vec![
            Span::styled(" /", Style::default().fg(t.accent)),
            Span::styled(app.query.clone(), Style::default().fg(t.text)),
        ];
        let used = 2 + w(&app.query);
        let tail = i18n::t("    enter 播放 · esc 取消", "    enter play · esc cancel");
        if used + w(tail) <= width {
            spans.push(Span::styled(tail, Style::default().fg(t.dim)));
        }
        Line::from(spans)
    } else if let Some((msg, _)) = &app.toast {
        Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(msg.clone(), Style::default().fg(t.accent2)),
        ])
    } else if let Some(err) = &app.err {
        Line::from(vec![
            Span::styled(" ! ", Style::default().fg(Color::Red)),
            Span::styled(
                fit(err, width.saturating_sub(4)),
                Style::default().fg(Color::Red),
            ),
        ])
    } else {
        hints_line(app, width)
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_loading(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spin = frames[(app.tick as usize / 2) % frames.len()];

    let (headline, detail) = match &app.failed {
        Some(e) => ("nothing to play".to_string(), e.clone()),
        None => (
            format!("{spin}  scanning library"),
            fit(&app.scan_file, area.width.saturating_sub(8) as usize),
        ),
    };

    let y = area.y + area.height / 2;
    let rect = Rect {
        x: area.x,
        y: y.saturating_sub(1),
        width: area.width,
        height: 2.min(area.height),
    };
    let lines = vec![
        Line::from(Span::styled(headline, Style::default().fg(t.accent)))
            .alignment(Alignment::Center),
        Line::from(Span::styled(detail, Style::default().fg(t.dim))).alignment(Alignment::Center),
    ];
    f.render_widget(Paragraph::new(lines), rect);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;

    let keys: &[(&str, &str)] = if i18n::is_en() {
        &[
            ("space", "play / pause"),
            ("enter", "play the selected track"),
            ("n / p", "next / previous track"),
            ("← / →", "seek back / forward 5 s"),
            ("j / k", "move down / up"),
            ("g / G", "jump to first / last"),
            ("1 2 3 4", "sequential reverse shuffle repeat-one"),
            ("m", "cycle the mode"),
            ("+ / -", "volume"),
            ("/", "search"),
            ("r", "clear the cover cache"),
            ("q", "quit"),
        ]
    } else {
        &[
            ("space", "播放 / 暂停"),
            ("enter", "播放选中曲目"),
            ("n / p", "下一首 / 上一首"),
            ("← / →", "后退 / 前进 5 秒"),
            ("j / k", "上下移动"),
            ("g / G", "跳到首 / 尾"),
            ("1 2 3 4", "顺序 倒序 随机 单曲"),
            ("m", "循环切换模式"),
            ("+ / -", "音量"),
            ("/", "搜索"),
            ("r", "清空封面缓存"),
            ("q", "退出"),
        ]
    };

    // Fit the panel to whatever column it was given: the narrow case is the
    // list beside the cover, and the rows above are short enough to survive it.
    let wdt = 58.min(area.width).max(16);
    let hgt = ((keys.len() as u16) + 6).min(area.height).max(6);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(wdt)) / 2,
        y: area.y + (area.height.saturating_sub(hgt)) / 2,
        width: wdt,
        height: hgt,
    };
    f.render_widget(Clear, rect);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    for (k, d) in keys {
        // pad by display width so CJK descriptions line up
        let pad = 8usize.saturating_sub(w(k));
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {k}{}", " ".repeat(pad)),
                Style::default().fg(t.accent),
            ),
            Span::styled(*d, Style::default().fg(t.muted)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "  {}: {}   cell: {}x{}px",
            i18n::t("封面", "cover"),
            app.graphics.proto.label(),
            app.caps.cell_w,
            app.caps.cell_h
        ),
        Style::default().fg(t.dim),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.accent))
        .title(Span::styled(" ymp · keys ", Style::default().fg(t.accent)))
        .style(Style::default().bg(t.sel_bg));

    f.render_widget(Paragraph::new(lines).block(block), rect);
}
