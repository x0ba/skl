//! ratatui widgets. Layout only — no I/O.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::app::{help_text, now_secs, App, Overlay};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_panes(frame, chunks[1], app);
    draw_footer(frame, chunks[2], app);

    if app.overlay == Overlay::Help {
        draw_help(frame, area);
    }
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let title = app.header_line(now_secs());
    let err = app
        .catalog
        .load_error
        .as_deref()
        .map(|e| format!("  ! {e}"))
        .unwrap_or_default();
    let para = Paragraph::new(format!("{title}{err}")).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" skl "),
    );
    frame.render_widget(para, area);
}

fn draw_panes(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    draw_list(frame, panes[0], app);
    draw_preview(frame, panes[1], app);
}

fn draw_list(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let idxs = app.filtered_indices();
    let items: Vec<ListItem> = idxs
        .iter()
        .map(|i| {
            let row = &app.catalog.skills[*i];
            let mark = if row.activated { '✓' } else { '·' };
            ListItem::new(format!("{mark} {}", row.name))
        })
        .collect();

    let title = if app.overlay == Overlay::Search {
        format!(" skills  /{} ", app.query)
    } else {
        " skills ".into()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !idxs.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_preview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(warn) = &app.preview.warning {
        lines.push(Line::from(Span::styled(
            warn.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
    }
    if app.preview.body.is_empty() && app.preview.warning.is_none() {
        if let Some(hint) = &app.catalog.empty_hint {
            for line in hint.lines() {
                lines.push(Line::from(line.to_string()));
            }
        }
    } else {
        for line in app.preview.body.lines() {
            lines.push(Line::from(line.to_string()));
        }
    }

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", app.preview.title)),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.preview_scroll, 0));
    frame.render_widget(para, area);
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let keys = if app.overlay == Overlay::Search {
        "type to filter   ↑↓ list   Enter done   Esc clear"
    } else {
        "/ search   ↑↓/jk list   [] preview   e edit   u use   U unuse   s sync   r refresh   ? help   q quit"
    };
    let status = if app.status.is_empty() {
        keys.to_string()
    } else {
        format!("{}   ·  {}", app.status, keys)
    };
    let para = Paragraph::new(status).block(Block::default().borders(Borders::ALL));
    frame.render_widget(para, area);
}

fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let popup = centered(area, 70, 80);
    let para = Paragraph::new(help_text())
        .block(Block::default().borders(Borders::ALL).title(" help "))
        .wrap(Wrap { trim: false });
    frame.render_widget(Clear, popup);
    frame.render_widget(para, popup);
}

fn centered(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(v[1])[1]
}
