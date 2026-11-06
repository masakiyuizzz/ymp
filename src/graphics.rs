use std::io::{self, Write};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use image::{imageops::FilterType, DynamicImage, ImageFormat};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Kitty,
    Iterm2,
    Blocks,
    Off,
}

impl Protocol {
    pub fn label(&self) -> &'static str {
        match self {
            Protocol::Kitty => "kitty",
            Protocol::Iterm2 => "iterm2",
            Protocol::Blocks => "blocks",
            Protocol::Off => "off",
        }
    }
}

pub struct Graphics {
    pub proto: Protocol,
    pub cell_w: u16,
    pub cell_h: u16,
    id: u32,
    last: Option<(u64, u16, u16)>,
    pub visible: bool,
}

impl Graphics {
    pub fn new(proto: Protocol, cell_w: u16, cell_h: u16) -> Self {
        Self {
            proto,
            cell_w: cell_w.max(1),
            cell_h: cell_h.max(1),
            id: 1,
            last: None,
            visible: true,
        }
    }

    pub fn is_graphics(&self) -> bool {
        matches!(self.proto, Protocol::Kitty | Protocol::Iterm2)
    }

    /// Forget what is on screen so the next present() redraws.
    pub fn invalidate(&mut self) {
        self.last = None;
    }

    pub fn delete<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        if !self.is_graphics() {
            return Ok(());
        }
        match self.proto {
            Protocol::Kitty => write!(out, "\x1b_Ga=d,d=i,i={}\x1b\\", self.id)?,
            Protocol::Iterm2 => {}
            _ => {}
        }
        self.last = None;
        Ok(())
    }

    pub fn clear_all<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        if self.proto == Protocol::Kitty {
            write!(out, "\x1b_Ga=d,d=A\x1b\\")?;
        }
        self.last = None;
        Ok(())
    }

    pub fn present<W: Write>(
        &mut self,
        out: &mut W,
        area: Rect,
        key: u64,
        img: Option<&DynamicImage>,
    ) -> io::Result<()> {
        if !self.is_graphics() || !self.visible || area.width < 2 || area.height < 1 {
            if self.last.is_some() {
                let _ = self.delete(out);
            }
            return Ok(());
        }
        let Some(img) = img else {
            if self.last.is_some() {
                let _ = self.delete(out);
            }
            return Ok(());
        };

        let cols = area.width;
        let rows = area.height;
        if self.last == Some((key, cols, rows)) {
            return Ok(());
        }

        let px_w = (cols as u32 * self.cell_w as u32).clamp(1, 4096);
        let px_h = (rows as u32 * self.cell_h as u32).clamp(1, 4096);
        let resized = img.resize_exact(px_w, px_h, FilterType::CatmullRom);

        let mut raw = Vec::new();
        {
            let mut cur = io::Cursor::new(&mut raw);
            if resized.write_to(&mut cur, ImageFormat::Png).is_err() {
                return Ok(());
            }
        }

        let _ = self.delete(out);
        self.id = self.id.wrapping_add(1).max(1);

        match self.proto {
            Protocol::Kitty => {
                write!(out, "\x1b[{};{}H", area.y + 1, area.x + 1)?;
                let b64 = STANDARD.encode(&raw);
                let mut chunks = b64.as_bytes().chunks(4096).peekable();
                let mut first = true;
                while let Some(chunk) = chunks.next() {
                    let more = if chunks.peek().is_some() { 1 } else { 0 };
                    if first {
                        write!(
                            out,
                            "\x1b_Ga=T,f=100,t=d,i={},C=1,c={},r={},q=2,m={};",
                            self.id, cols, rows, more
                        )?;
                        first = false;
                    } else {
                        write!(out, "\x1b_Gm={};", more)?;
                    }
                    out.write_all(chunk)?;
                    write!(out, "\x1b\\")?;
                }
            }
            Protocol::Iterm2 => {
                write!(out, "\x1b[{};{}H", area.y + 1, area.x + 1)?;
                let b64 = STANDARD.encode(&raw);
                write!(
                    out,
                    "\x1b]1337;File=inline=1;width={};height={};preserveAspectRatio=0;doNotMoveCursor=1:{}\x07",
                    cols, rows, b64
                )?;
            }
            _ => {}
        }
        out.flush()?;
        self.last = Some((key, cols, rows));
        Ok(())
    }

    /// Render the cover with half block characters: two pixels per cell.
    pub fn blocks(
        &self,
        img: &DynamicImage,
        cols: u16,
        rows: u16,
        dim: bool,
    ) -> Vec<Line<'static>> {
        if cols == 0 || rows == 0 {
            return Vec::new();
        }
        let px_w = cols as u32;
        let px_h = rows as u32 * 2;
        let small = img.resize_exact(px_w, px_h, FilterType::Triangle).to_rgba8();
        let mut lines = Vec::with_capacity(rows as usize);
        for r in 0..rows as u32 {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(cols as usize);
            for c in 0..cols as u32 {
                let top = small.get_pixel(c, r * 2);
                let bot = small.get_pixel(c, (r * 2 + 1).min(px_h - 1));
                let mut fg = Color::Rgb(top[0], top[1], top[2]);
                let mut bg = Color::Rgb(bot[0], bot[1], bot[2]);
                if dim {
                    fg = scale(fg, 0.55);
                    bg = scale(bg, 0.55);
                }
                spans.push(Span::styled("▀", Style::default().fg(fg).bg(bg)));
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

fn scale(c: Color, f: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * f) as u8,
            (g as f32 * f) as u8,
            (b as f32 * f) as u8,
        ),
        other => other,
    }
}

/// A neutral placeholder shown when a track has no artwork.
pub fn placeholder(cols: u16, rows: u16, accent: Color, dim: Color) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let inner_w = cols.saturating_sub(2) as usize;
    for r in 0..rows {
        let mut s = String::new();
        if r == 0 || r + 1 == rows {
            s.push(' ');
            s.push_str(&"·".repeat(inner_w.max(1)));
            s.push(' ');
        } else if r == rows / 2 && rows > 4 {
            let label = " no artwork ";
            let pad = inner_w.saturating_sub(label.chars().count()) / 2;
            s.push(' ');
            s.push_str(&" ".repeat(pad));
            s.push_str(label);
            s.push_str(
                &" ".repeat(inner_w.saturating_sub(pad + label.chars().count())),
            );
            s.push(' ');
        } else {
            s.push(' ');
            s.push_str(&" ".repeat(inner_w.max(1)));
            s.push(' ');
        }
        let style = if r == rows / 2 && rows > 4 {
            Style::default().fg(accent)
        } else {
            Style::default().fg(dim)
        };
        lines.push(Line::from(Span::styled(s, style)));
    }
    lines
}
