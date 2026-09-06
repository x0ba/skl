//! ratatui widgets. Layout only — no I/O.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::app::{help_text, now_secs, App, Overlay};

/// One footer binding. Labels drop first when the terminal is too narrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyHint {
    pub key: &'static str,
    pub label: &'static str,
}

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
    if app.overlay == Overlay::Create {
        draw_create(frame, area, app);
    }
    if app.overlay == Overlay::ConfirmDelete {
        draw_confirm(frame, area, app);
    }
    if let Some(message) = app.visible_toast() {
        draw_toast(frame, area, message);
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
    let para = Paragraph::new(format!("{title}{err}"))
        .block(Block::default().borders(Borders::ALL).title(" skl "));
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
    let inner = area.width.saturating_sub(2) as usize;
    let keys = fit_hints(footer_hints(app.overlay), inner);
    let para = Paragraph::new(keys).block(Block::default().borders(Borders::ALL));
    frame.render_widget(para, area);
}

fn footer_hints(overlay: Overlay) -> &'static [KeyHint] {
    match overlay {
        Overlay::Search => &[
            KeyHint {
                key: "↑↓",
                label: "move",
            },
            KeyHint {
                key: "Enter",
                label: "done",
            },
            KeyHint {
                key: "Esc",
                label: "clear",
            },
        ],
        Overlay::Create => &[
            KeyHint {
                key: "Enter",
                label: "create",
            },
            KeyHint {
                key: "Esc",
                label: "cancel",
            },
        ],
        Overlay::ConfirmDelete => &[
            KeyHint {
                key: "y",
                label: "confirm",
            },
            KeyHint {
                key: "n/Esc",
                label: "cancel",
            },
        ],
        Overlay::None | Overlay::Help => &[
            KeyHint {
                key: "/",
                label: "search",
            },
            KeyHint {
                key: "n",
                label: "new",
            },
            KeyHint {
                key: "↑↓/jk",
                label: "move",
            },
            KeyHint {
                key: "[]",
                label: "scroll",
            },
            KeyHint {
                key: "e",
                label: "edit",
            },
            KeyHint {
                key: "u/U",
                label: "use",
            },
            KeyHint {
                key: "d",
                label: "delete",
            },
            KeyHint {
                key: "s",
                label: "sync",
            },
            KeyHint {
                key: "r",
                label: "refresh",
            },
            KeyHint {
                key: "?",
                label: "help",
            },
            KeyHint {
                key: "q",
                label: "quit",
            },
        ],
    }
}

/// Prefer `key label` pairs. If they overflow, drop labels, then drop trailing keys.
pub fn fit_hints(hints: &[KeyHint], width: usize) -> String {
    if width == 0 || hints.is_empty() {
        return String::new();
    }
    let labeled = join_hints(hints, true);
    if display_len(&labeled) <= width {
        return labeled;
    }
    let compact = join_hints(hints, false);
    if display_len(&compact) <= width {
        return compact;
    }
    for take in (1..hints.len()).rev() {
        let line = join_hints(&hints[..take], false);
        if display_len(&line) <= width {
            return line;
        }
    }
    truncate_chars(hints[0].key, width)
}

fn join_hints(hints: &[KeyHint], with_labels: bool) -> String {
    hints
        .iter()
        .map(|hint| {
            if with_labels {
                format!("{} {}", hint.key, hint.label)
            } else {
                hint.key.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn display_len(s: &str) -> usize {
    s.chars().count()
}

fn truncate_chars(s: &str, width: usize) -> String {
    s.chars().take(width).collect()
}

fn draw_create(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let popup = centered(area, 54, 28);
    let name = if app.create_name.is_empty() {
        "_".to_string()
    } else {
        format!("{}_", app.create_name)
    };
    let body = format!("name: {name}\n\nEnter opens $EDITOR with name + description frontmatter.");
    let para = Paragraph::new(body)
        .block(Block::default().borders(Borders::ALL).title(" new skill "))
        .wrap(Wrap { trim: false });
    frame.render_widget(Clear, popup);
    frame.render_widget(para, popup);
}

fn draw_confirm(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let name = app
        .selected_row()
        .map(|row| row.name.as_str())
        .unwrap_or("skill");
    let heading = format!("Delete {name}?");
    let body = format!("{heading}\n\nRemoves the library copy.");
    let width =
        ((heading.chars().count() as u16) + 4).clamp(36, area.width.saturating_sub(4).max(20));
    let popup = popup_sized(area, width, 7);
    let para = Paragraph::new(body)
        .block(Block::default().borders(Borders::ALL).title(" delete "))
        .wrap(Wrap { trim: false });
    frame.render_widget(Clear, popup);
    frame.render_widget(para, popup);
}

fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let popup = centered(area, 70, 80);
    let para = Paragraph::new(help_text())
        .block(Block::default().borders(Borders::ALL).title(" help "))
        .wrap(Wrap { trim: false });
    frame.render_widget(Clear, popup);
    frame.render_widget(para, popup);
}

fn draw_toast(frame: &mut Frame<'_>, area: Rect, message: &str) {
    let (width, height) = toast_dims(message, area);
    let popup = toast_rect(area, width, height);
    let para = Paragraph::new(message)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(Clear, popup);
    frame.render_widget(para, popup);
}

fn toast_dims(message: &str, area: Rect) -> (u16, u16) {
    let max_inner = (area.width / 2).clamp(20, 56);
    let chars = message.chars().count() as u16;
    let inner = chars.min(max_inner).max(8);
    let text_lines = chars.div_ceil(inner).max(1);
    let height = (text_lines + 2).min(6);
    (inner.saturating_add(2), height)
}

/// Bottom-right, sitting on the pane above the footer.
fn toast_rect(area: Rect, width: u16, height: u16) -> Rect {
    let footer = 3;
    let width = width.min(area.width.saturating_sub(2)).max(1);
    let height = height.min(area.height.saturating_sub(footer)).max(1);
    Rect {
        x: area.x + area.width.saturating_sub(width + 1),
        y: area.y + area.height.saturating_sub(footer + height),
        width,
        height,
    }
}

fn popup_sized(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width).max(1);
    let height = height.min(area.height).max(1);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const BROWSE: &[KeyHint] = &[
        KeyHint {
            key: "/",
            label: "search",
        },
        KeyHint {
            key: "n",
            label: "new",
        },
        KeyHint {
            key: "↑↓/jk",
            label: "move",
        },
        KeyHint {
            key: "[]",
            label: "scroll",
        },
        KeyHint {
            key: "e",
            label: "edit",
        },
        KeyHint {
            key: "u/U",
            label: "use",
        },
        KeyHint {
            key: "d",
            label: "delete",
        },
        KeyHint {
            key: "s",
            label: "sync",
        },
        KeyHint {
            key: "r",
            label: "refresh",
        },
        KeyHint {
            key: "?",
            label: "help",
        },
        KeyHint {
            key: "q",
            label: "quit",
        },
    ];

    #[test]
    fn wide_footer_keeps_labels() {
        let line = fit_hints(BROWSE, 120);
        assert!(line.contains("/ search"), "{line}");
        assert!(line.contains("q quit"), "{line}");
        assert!(line.contains("refresh"), "{line}");
    }

    #[test]
    fn mid_footer_drops_labels_not_keys() {
        let line = fit_hints(BROWSE, 40);
        assert!(!line.contains("search"), "{line}");
        assert!(!line.contains("refresh"), "{line}");
        assert!(line.contains("/"), "{line}");
        assert!(line.contains("q"), "{line}");
        assert!(line.contains("↑↓/jk"), "{line}");
        assert!(display_len(&line) <= 40, "{line}");
    }

    #[test]
    fn narrow_footer_drops_trailing_keys() {
        let line = fit_hints(BROWSE, 16);
        assert!(display_len(&line) <= 16, "{line}");
        assert!(line.contains('/'), "{line}");
        assert!(!line.contains("search"), "{line}");
        assert!(!line.contains('q'), "{line}");
    }
}
