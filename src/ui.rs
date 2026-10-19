use crate::{
    app::{App, Panel},
    graphics::{Images, Placement},
    layout::{self, ACCENT, INK, MUTED, WARN},
    source,
    terminal::{Guard, Input, InputEvent},
};
use anyhow::Result;
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    terminal,
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::{
    io::{self, Write},
    sync::atomic::Ordering,
    time::Duration,
};

const BG: Color = Color::Rgb(17, 24, 32);
const BAR: Color = Color::Rgb(28, 38, 49);
pub const HELP: &str = "tpreview · keyboard guide\n\nj / k  ↑ / ↓       Scroll (or move in focused panel)\nPageUp / PageDown   Page up / down; Space: page down\ng / G              First / last page\nh / l  ← / →       Pan tables, code and text diagrams\n/                  Search; Enter: jump; Esc: close input\nn / N              Next / previous search match\nTab / Shift+Tab    Select next / previous link or diagram\nEnter              Open selected item\nb / Backspace      Previous document\nf                  Focus/toggle files (j/k, Enter or l to open)\nt                  Toggle contents (j/k, Enter to jump)\nEsc                Close panel / selection / search marks\nr                  Reload current file\n?                  This guide; Esc or ?: close\nq / Ctrl+C         Quit (q closes a detail/guide first)\n\nDiagram detail: + / - zoom image; 0 reset\nh/j/k/l or arrows: pan; PageUp/Down: vertical pan\nEsc or q: return to document\n\nClick links or use Tab, Enter to open.\nHTTP/HTTPS: browser; local paths: document.\nClick a file to open it; Shift+drag selects terminal text.";

pub fn run(mut app: App, mut input: Input, _guard: Guard) -> Result<()> {
    let stop = _guard.stop.clone();
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut images = Images::default();
    let mut size = (0, 0);
    let result = (|| -> Result<()> {
        while !app.quit && !stop.load(Ordering::Relaxed) {
            app.tick();
            let new_size = terminal::size()?;
            if new_size != size {
                size = new_size;
                app.dirty = true;
            }
            let area = Rect::new(0, 0, size.0, size.1);
            let (_, body, _) = areas(area, &app);
            app.resize(body.width as usize, body.height as usize);
            if app.dirty {
                images.clear(terminal.backend_mut())?;
                let mut placements = Vec::new();
                terminal.draw(|f| placements = draw(f, &app))?;
                images.display(&placements, terminal.backend_mut())?;
                terminal.backend_mut().flush()?;
                app.dirty = false;
            }
            if let Some(key) = input.pending.pop_front() {
                app.key(key);
                continue;
            }
            match input.poll(Duration::from_millis(35))? {
                Some(InputEvent::Key(key)) => app.key(key),
                Some(InputEvent::Mouse(event)) => mouse(&mut app, area, event),
                _ => {}
            }
        }
        Ok(())
    })();
    images.clear(terminal.backend_mut())?;
    result
}
fn areas(area: Rect, app: &App) -> (Option<Rect>, Rect, Rect) {
    let status = Rect::new(
        0,
        area.height.saturating_sub(2),
        area.width,
        2.min(area.height),
    );
    let content = Rect::new(
        0,
        2.min(area.height),
        area.width,
        area.height.saturating_sub(4),
    );
    let panel = if app.panel.is_some()
        && !app.help
        && app.detail.is_none()
        && (area.width >= 60 || app.panel_focus)
    {
        Some(Rect::new(
            0,
            content.y,
            if area.width < 60 {
                area.width
            } else {
                28.min(area.width / 3)
            },
            content.height,
        ))
    } else {
        None
    };
    let start = if area.width < 60 {
        0
    } else {
        panel.map_or(0, |r| r.width)
    };
    let width = area.width.saturating_sub(start);
    // Quiet, readable measure; wide blocks remain accessible through horizontal pan.
    let inset = if width > 116 {
        (width - 112) / 2
    } else {
        2.min(width / 2)
    };
    let body = Rect::new(
        start + inset,
        content.y,
        width.saturating_sub(inset * 2),
        content.height,
    );
    (panel, body, status)
}
pub fn mouse(app: &mut App, area: Rect, event: MouseEvent) {
    let (panel, body, _) = areas(area, app);
    let point = ratatui::layout::Position::new(event.column, event.row);
    if matches!(
        event.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
    ) {
        if !app.help && app.detail.is_none() {
            app.panel_focus = panel.is_some_and(|rect| rect.contains(point));
        }
        let key = if event.kind == MouseEventKind::ScrollUp {
            KeyCode::Up
        } else {
            KeyCode::Down
        };
        for _ in 0..3 {
            app.key(KeyEvent::new(key, KeyModifiers::NONE));
        }
        return;
    }
    if event.kind != MouseEventKind::Down(MouseButton::Left)
        || app.help
        || app.detail.is_some()
        || app.searching
    {
        return;
    }
    if let Some(rect) = panel.filter(|rect| rect.contains(point)) {
        app.panel_focus = true;
        app.dirty = true;
        let page = rect.height.saturating_sub(2).max(1) as usize;
        let start = app.panel_index.saturating_sub(page - 1);
        let row = event.row.saturating_sub(rect.y) as usize;
        if row > 0 && row <= page && event.column < rect.right().saturating_sub(1) {
            let index = start + row - 1;
            let len = if app.panel == Some(Panel::Files) {
                app.files.len()
            } else {
                app.headings().len()
            };
            if index < len {
                app.panel_index = index;
                app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            }
        }
        return;
    }
    if !body.contains(point) {
        return;
    }
    app.panel_focus = false;
    app.dirty = true;
    if let Some(row) = app
        .layout
        .rows
        .get(app.scroll + (event.row - body.y) as usize)
    {
        let column = (event.column - body.x) as usize + if row.wide { app.horizontal } else { 0 };
        let target = row
            .links
            .iter()
            .find(|(range, _)| range.contains(&column))
            .map(|(_, index)| app.doc.blocks[row.block].links[*index].target.clone());
        if let Some(target) = target {
            app.open_target(&target);
        }
    }
}
fn placement(app: &App, block: usize, rect: Rect, crop: (u32, u32, u32, u32)) -> Option<Placement> {
    let d = app.diagrams.get(&block)?.good.as_ref()?;
    Some(Placement {
        id: 1000 + (std::process::id() % 1_000_000) * 64 + block as u32,
        png: d.png.clone()?,
        x: rect.x,
        y: rect.y,
        cols: rect.width,
        rows: rect.height,
        crop,
    })
}
pub fn draw(frame: &mut ratatui::Frame, app: &App) -> Vec<Placement> {
    let area = frame.area();
    let (panel, body, status) = areas(area, app);
    let mut images = Vec::new();
    frame.render_widget(
        Block::default().style(Style::default().bg(BG).fg(INK)),
        area,
    );
    let name = app
        .path
        .as_ref()
        .map(|p| p.strip_prefix(&app.root).unwrap_or(p).display().to_string())
        .unwrap_or_else(|| "stdin".into());
    let header = Line::from(vec![
        Span::styled(
            " tpreview ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" / ", Style::default().fg(MUTED)),
        Span::raw(source::safe(&name)),
    ]);
    frame.render_widget(
        Paragraph::new(header).style(Style::default().bg(BAR)),
        Rect::new(0, 0, area.width, 1.min(area.height)),
    );
    if let Some(rect) = panel {
        let mut lines = Vec::new();
        let title = if app.panel == Some(Panel::Files) {
            "FILES"
        } else {
            "CONTENTS"
        };
        lines.push(Line::styled(
            format!(
                " {title}{}",
                if app.panel_focus { " · focused" } else { "" }
            ),
            Style::default().fg(ACCENT),
        ));
        let entries: Vec<String> = if app.panel == Some(Panel::Files) {
            app.files
                .iter()
                .map(|p| {
                    source::safe(&p.strip_prefix(&app.root).unwrap_or(p).display().to_string())
                })
                .collect()
        } else {
            app.headings().into_iter().map(|(_, s)| s).collect()
        };
        let page = rect.height.saturating_sub(2).max(1) as usize;
        let start = app.panel_index.saturating_sub(page - 1);
        for (i, name) in entries.into_iter().enumerate().skip(start).take(page) {
            lines.push(Line::styled(
                format!("{}{name}", if i == app.panel_index { " › " } else { "   " }),
                Style::default().fg(if i == app.panel_index { ACCENT } else { MUTED }),
            ));
        }
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::RIGHT)
                    .border_style(Style::default().fg(BAR)),
            ),
            rect,
        );
    }
    if panel.is_some() && area.width < 60 {
        // Narrow screens show the focused panel as an overlay.
    } else if app.help {
        frame.render_widget(
            Paragraph::new(
                layout::wrap(HELP, body.width as usize)
                    .into_iter()
                    .skip(app.help_scroll)
                    .map(Line::raw)
                    .collect::<Vec<_>>(),
            )
            .style(Style::default().fg(INK)),
            body,
        );
    } else if let Some(detail) = &app.detail {
        frame.render_widget(
            Paragraph::new(format!(
                "◇ Mermaid · {:.0}% · +/- zoom · arrows pan · Esc back",
                detail.zoom * 100.
            ))
            .style(Style::default().fg(ACCENT)),
            Rect::new(body.x, body.y, body.width, 1.min(body.height)),
        );
        let rect = Rect::new(
            body.x,
            body.y + 1,
            body.width,
            body.height.saturating_sub(1),
        );
        if let Some(d) = app
            .diagrams
            .get(&detail.block)
            .and_then(|s| s.good.as_ref())
        {
            if app.cap.image && d.png.is_some() && rect.width > 0 && rect.height > 0 {
                let cols = (rect.width as f64 * detail.zoom).round().max(1.) as usize;
                let rows =
                    ((d.height as f64 / d.width as f64) * cols as f64 * app.cap.cell.0 as f64
                        / app.cap.cell.1 as f64)
                        .ceil()
                        .max(1.) as usize;
                let x = detail.x.min(cols.saturating_sub(1));
                let y = detail.y.min(rows.saturating_sub(1));
                let visible_cols = (cols - x).min(rect.width as usize);
                let visible_rows = (rows - y).min(rect.height as usize);
                let crop = (
                    (x as f64 / cols as f64 * d.width as f64) as u32,
                    (y as f64 / rows as f64 * d.height as f64) as u32,
                    (visible_cols as f64 / cols as f64 * d.width as f64).ceil() as u32,
                    (visible_rows as f64 / rows as f64 * d.height as f64).ceil() as u32,
                );
                if let Some(p) = placement(
                    app,
                    detail.block,
                    Rect::new(rect.x, rect.y, visible_cols as u16, visible_rows as u16),
                    crop,
                ) {
                    images.push(p);
                }
            } else {
                let lines: Vec<_> = d
                    .text
                    .lines()
                    .skip(detail.y)
                    .take(rect.height as usize)
                    .map(|s| layout::crop(&Line::raw(s.to_owned()), detail.x, rect.width as usize))
                    .collect();
                frame.render_widget(
                    Paragraph::new(lines).style(Style::default().fg(ACCENT)),
                    rect,
                );
            }
        } else if let Some(b) = app.doc.blocks.get(detail.block) {
            let lines: Vec<_> = b
                .text
                .lines()
                .skip(detail.y)
                .take(rect.height as usize)
                .map(|s| layout::crop(&Line::raw(s.to_owned()), detail.x, rect.width as usize))
                .collect();
            frame.render_widget(Paragraph::new(lines), rect);
        }
    } else {
        let selected = app
            .selected
            .and_then(|i| app.actions.get(i))
            .map(|a| a.block);
        for (n, row) in app
            .layout
            .rows
            .iter()
            .skip(app.scroll)
            .take(body.height as usize)
            .enumerate()
        {
            let mut line = layout::crop(
                &row.line,
                if row.wide { app.horizontal } else { 0 },
                body.width as usize,
            );
            if selected == Some(row.block) {
                line = line.style(Style::default().bg(BAR));
            }
            if app.matches.binary_search(&(app.scroll + n)).is_ok() {
                line = line.style(
                    Style::default()
                        .bg(Color::Rgb(75, 60, 28))
                        .fg(Color::Rgb(255, 226, 153)),
                );
            }
            frame.render_widget(
                Paragraph::new(line),
                Rect::new(body.x, body.y + n as u16, body.width, 1),
            );
            if let Some((y, total_rows, total_cols)) = row.image {
                // One clipped image per visible diagram, including partial top/bottom scroll.
                if (y == 0 || n == 0)
                    && let Some(d) = app.diagrams.get(&row.block).and_then(|s| s.good.as_ref())
                {
                    let x = app.horizontal.min(total_cols);
                    let rows = (total_rows - y).min(body.height as usize - n);
                    let cols = total_cols.saturating_sub(x).min(body.width as usize);
                    if cols > 0 && rows > 0 {
                        let crop = (
                            (x as f64 / total_cols as f64 * d.width as f64) as u32,
                            (y as f64 / total_rows as f64 * d.height as f64) as u32,
                            (cols as f64 / total_cols as f64 * d.width as f64).ceil() as u32,
                            (rows as f64 / total_rows as f64 * d.height as f64).ceil() as u32,
                        );
                        if let Some(p) = placement(
                            app,
                            row.block,
                            Rect::new(body.x, body.y + n as u16, cols as u16, rows as u16),
                            crop,
                        ) {
                            images.push(p);
                        }
                    }
                }
            }
        }
    }
    let errors = app
        .diagrams
        .values()
        .filter(|s| s.error.is_some() || s.good.as_ref().is_some_and(|d| d.image_error.is_some()))
        .count();
    let pending = app.diagrams.values().filter(|s| s.pending).count();
    let watch = if app.watch.is_some() {
        "watch:on"
    } else {
        "watch:off"
    };
    let mode = if app.cap.image { "image/Kitty" } else { "text" };
    let filename = source::safe(
        &app.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "stdin".into()),
    );
    let mut bottom = vec![Line::styled(
        format!(
            " {filename} · {watch} · {mode} · {} / {} · {errors} errors · {pending} rendering · x:{}",
            app.scroll + usize::from(!app.layout.rows.is_empty()),
            app.layout.rows.len(),
            app.horizontal
        ),
        Style::default()
            .bg(BAR)
            .fg(if errors > 0 { WARN } else { MUTED }),
    )];
    let hint = if app.help {
        " j/k or PageUp/Down: scroll guide · Esc: back".into()
    } else if app.searching {
        format!(" /{}▏  {} matches", app.query, app.matches.len())
    } else if let Some(a) = app.selected.and_then(|i| app.actions.get(i)) {
        format!(" Enter: {}   Esc: clear", a.label)
    } else if app.message.starts_with("read error:") || app.message.starts_with("watch error:") {
        format!(" {}", app.message)
    } else {
        format!(
            " ?:help  f:files  t:contents  /:search  Tab:select  q:quit   {}",
            app.message
        )
    };
    bottom.push(Line::styled(
        source::safe(&hint),
        Style::default().fg(MUTED),
    ));
    frame.render_widget(Clear, status);
    frame.render_widget(
        Paragraph::new(bottom).style(Style::default().bg(BG)),
        status,
    );
    images
}
