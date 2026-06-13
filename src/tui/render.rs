use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::model::{format_count, format_size};

use super::colors::{bar_color, depth_bg, depth_fg, size_color};
use super::row::VisibleRow;
use super::state::{App, DeleteState};

impl App {
    pub(super) fn render(&self, frame: &mut Frame) {
        let rows = self.visible_rows();
        let area = frame.area();

        let layout = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

        self.render_header(frame, layout[0]);
        self.render_tree(frame, layout[1], &rows);
        self.render_info(frame, layout[2], &rows);
        self.render_help(frame, layout[3]);

        if let DeleteState::Confirm {
            ref path,
            ref name,
            size,
            is_root,
            is_file,
        } = self.delete_state
        {
            self.render_confirm_dialog(frame, path, name, size, is_root, is_file);
        }

        if let DeleteState::PendingD = self.delete_state {
            self.render_pending_d(frame);
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title_line = Line::from(vec![
            Span::styled("  Dir Analyzer", Style::default().fg(Color::Cyan).bold()),
            Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                self.root.path.display().to_string(),
                Style::default().fg(Color::White).bold(),
            ),
            Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_size(self.root.total_size),
                Style::default().fg(Color::Yellow).bold(),
            ),
            Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} files", format_count(self.root.file_count)),
                Style::default().fg(Color::White),
            ),
            Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} dirs", format_count(self.root.dir_count)),
                Style::default().fg(Color::White),
            ),
        ]);

        let w = area.width as usize;
        let sep = "─".repeat(w);
        let separator = Line::styled(&*sep, Style::default().fg(Color::DarkGray));

        let tree_label = "  Lvl  Tree / Name";
        let right_cols = "     Size  % of Parent          % of Root";
        let pad = w.saturating_sub(tree_label.len() + right_cols.len());
        let col_header = Line::from(vec![
            Span::styled(tree_label, Style::default().fg(Color::DarkGray)),
            Span::raw(" ".repeat(pad)),
            Span::styled(right_cols, Style::default().fg(Color::DarkGray)),
        ]);

        let header = Paragraph::new(vec![title_line, separator, col_header]);
        frame.render_widget(header, area);
    }

    fn render_tree(&self, frame: &mut Frame, area: Rect, rows: &[VisibleRow]) {
        let width = area.width as usize;

        let items: Vec<ListItem> = rows
            .iter()
            .map(|row| {
                let line = build_tree_line(row, width);
                ListItem::new(line).bg(depth_bg(row.depth))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(50, 50, 80))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(" ");

        let mut state = ListState::default();
        state.select(Some(self.cursor));
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_info(&self, frame: &mut Frame, area: Rect, rows: &[VisibleRow]) {
        let line = if let Some(row) = rows.get(self.cursor) {
            if row.is_file_cutoff {
                let is_expandable = !self.show_all_files.contains(&row.path);
                let hint = if is_expandable {
                    "Press Enter or → to show more"
                } else {
                    "These files were too small to track individually"
                };
                Line::from(vec![
                    Span::styled(
                        format!(" L{} ", row.depth),
                        Style::default()
                            .fg(Color::Black)
                            .bg(depth_fg(row.depth))
                            .bold(),
                    ),
                    Span::styled(
                        format!("  {}  │  ", row.name),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        hint,
                        Style::default().fg(if is_expandable {
                            Color::Cyan
                        } else {
                            Color::DarkGray
                        }),
                    ),
                ])
            } else {
                let pct_parent = if row.parent_size > 0 {
                    row.total_size as f64 / row.parent_size as f64 * 100.0
                } else {
                    100.0
                };
                let pct_root = if row.root_size > 0 {
                    row.total_size as f64 / row.root_size as f64 * 100.0
                } else {
                    100.0
                };

                let kind_label = if row.is_file { "file" } else { "dir" };

                let mut spans = vec![
                    Span::styled(
                        format!(" L{} ", row.depth),
                        Style::default()
                            .fg(Color::Black)
                            .bg(depth_fg(row.depth))
                            .bold(),
                    ),
                    Span::styled(
                        format!(" {} ", kind_label),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(" → ", Style::default().fg(Color::Yellow)),
                    Span::styled(
                        row.path.display().to_string(),
                        Style::default().fg(Color::White).bold(),
                    ),
                    Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format_size(row.total_size),
                        Style::default().fg(size_color(row.total_size)),
                    ),
                ];

                if !row.is_file {
                    spans.push(Span::styled(
                        format!(" (own: {})", format_size(row.own_size)),
                        Style::default().fg(Color::DarkGray),
                    ));
                    spans.push(Span::styled("  │  ", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled(
                        format!(
                            "{} files, {} dirs",
                            format_count(row.file_count),
                            format_count(row.dir_count)
                        ),
                        Style::default().fg(Color::White),
                    ));
                }

                spans.extend([
                    Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{:.1}% parent", pct_parent),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{:.1}% root", pct_root),
                        Style::default().fg(Color::Yellow),
                    ),
                ]);

                Line::from(spans)
            }
        } else {
            Line::raw("")
        };

        let info = Paragraph::new(line).style(Style::default().bg(Color::Rgb(30, 30, 40)));
        frame.render_widget(info, area);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        if let Some(ref msg) = self.status {
            if msg.created.elapsed().as_secs() < 3 {
                let line = Line::from(vec![Span::styled(format!("  {}", msg.text), msg.style)]);
                let bar = Paragraph::new(line).style(Style::default().bg(Color::Rgb(25, 25, 35)));
                frame.render_widget(bar, area);
                return;
            }
        }

        let help = Line::from(vec![
            Span::styled(" ↑↓/jk", Style::default().fg(Color::Yellow)),
            Span::styled(" Nav  ", Style::default().fg(Color::DarkGray)),
            Span::styled("←→/hl", Style::default().fg(Color::Yellow)),
            Span::styled(" Expand  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::styled(" Toggle  ", Style::default().fg(Color::DarkGray)),
            Span::styled("e", Style::default().fg(Color::Yellow)),
            Span::styled(" Expand All  ", Style::default().fg(Color::DarkGray)),
            Span::styled("c", Style::default().fg(Color::Yellow)),
            Span::styled(" Collapse All  ", Style::default().fg(Color::DarkGray)),
            Span::styled("dd", Style::default().fg(Color::Red)),
            Span::styled(" Delete  ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::styled(" Quit", Style::default().fg(Color::DarkGray)),
        ]);

        let bar = Paragraph::new(help).style(Style::default().bg(Color::Rgb(25, 25, 35)));
        frame.render_widget(bar, area);
    }

    fn render_pending_d(&self, frame: &mut Frame) {
        let area = frame.area();
        let w: u16 = 38;
        let popup = Rect {
            x: area.width.saturating_sub(w + 1),
            y: area.height.saturating_sub(2),
            width: w.min(area.width),
            height: 1,
        };
        let msg = Paragraph::new(Line::from(vec![
            Span::styled(" Press ", Style::default().fg(Color::DarkGray)),
            Span::styled("d", Style::default().fg(Color::Red).bold()),
            Span::styled(
                " again to delete selected ",
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .style(Style::default().bg(Color::Rgb(60, 30, 30)));
        frame.render_widget(msg, popup);
    }

    fn render_confirm_dialog(
        &self,
        frame: &mut Frame,
        path: &std::path::Path,
        name: &str,
        size: u64,
        is_root: bool,
        is_file: bool,
    ) {
        let area = frame.area();
        let popup_w = 64.min(area.width.saturating_sub(4));
        let popup_h: u16 = if is_root { 6 } else { 7 };
        let popup = Rect {
            x: (area.width.saturating_sub(popup_w)) / 2,
            y: (area.height.saturating_sub(popup_h)) / 2,
            width: popup_w,
            height: popup_h,
        };

        frame.render_widget(Clear, popup);

        let mut lines = vec![Line::raw("")];

        if is_root {
            lines.push(Line::styled(
                "  Cannot delete the root scan directory.",
                Style::default().fg(Color::Red).bold(),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled("  Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("any key", Style::default().fg(Color::Yellow).bold()),
                Span::styled(" to dismiss", Style::default().fg(Color::DarkGray)),
            ]));
        } else {
            let path_str = path.display().to_string();
            let max_path = (popup_w as usize).saturating_sub(6);
            let display_path = if path_str.len() > max_path {
                format!("…{}", &path_str[path_str.len() - max_path + 1..])
            } else {
                path_str
            };
            let kind = if is_file { "file" } else { "directory" };
            let display_name = if is_file {
                name.to_string()
            } else {
                format!("{}/", name)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  Delete {} ", kind),
                    Style::default().fg(Color::Red).bold(),
                ),
                Span::styled(display_name, Style::default().fg(Color::White).bold()),
                Span::styled(
                    format!(" ({})?", format_size(size)),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
            lines.push(Line::styled(
                format!("  {}", display_path),
                Style::default().fg(Color::DarkGray),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "  WARNING: This is permanent and cannot be undone!",
                Style::default().fg(Color::Red),
            ));
            lines.push(Line::from(vec![
                Span::styled("  Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("y", Style::default().fg(Color::Green).bold()),
                Span::styled(" to confirm, ", Style::default().fg(Color::DarkGray)),
                Span::styled("any other key", Style::default().fg(Color::Cyan).bold()),
                Span::styled(" to cancel", Style::default().fg(Color::DarkGray)),
            ]));
        }

        let block = Block::default()
            .title(" ⚠  Confirm Delete ")
            .title_style(Style::default().fg(Color::Red).bold())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .style(Style::default().bg(Color::Rgb(40, 20, 20)));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup);
    }
}

// ── Tree line builder ───────────────────────────────────────────────────────

fn build_tree_line(row: &VisibleRow, width: usize) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut prefix_len: usize = 0;

    let dim = Style::default().fg(Color::DarkGray);

    // Depth level tag
    let depth_tag = format!(" L{} ", row.depth);
    if row.is_file_cutoff {
        spans.push(Span::styled(depth_tag.clone(), dim));
    } else {
        spans.push(Span::styled(
            depth_tag.clone(),
            Style::default()
                .fg(Color::Black)
                .bg(depth_fg(row.depth))
                .bold(),
        ));
    }
    prefix_len += depth_tag.len();

    spans.push(Span::raw(" "));
    prefix_len += 1;

    // Tree connector lines
    if row.depth > 0 {
        for i in 0..row.ancestor_is_last.len() {
            if i == 0 {
                continue;
            }
            let seg_color = if row.is_file_cutoff {
                dim
            } else {
                Style::default().fg(depth_fg(i))
            };
            if row.ancestor_is_last[i] {
                spans.push(Span::styled("    ", seg_color));
            } else {
                spans.push(Span::styled(" │  ", seg_color));
            }
            prefix_len += 4;
        }
        let conn_color = if row.is_file_cutoff {
            dim
        } else {
            Style::default().fg(depth_fg(row.depth))
        };
        if row.is_last {
            spans.push(Span::styled(" └─ ", conn_color));
        } else {
            spans.push(Span::styled(" ├─ ", conn_color));
        }
        prefix_len += 4;
    }

    // Cutoff rows get a fully dim line
    if row.is_file_cutoff {
        spans.push(Span::styled("  ", dim));
        prefix_len += 2;

        let remaining = width.saturating_sub(prefix_len);
        let label = &row.name;
        let truncated = if label.len() > remaining {
            format!("{}…", &label[..remaining.saturating_sub(1)])
        } else {
            format!("{:<width$}", label, width = remaining)
        };
        spans.push(Span::styled(
            truncated,
            Style::default().fg(Color::Rgb(100, 100, 130)),
        ));
        return Line::from(spans);
    }

    // Icon column (2 chars)
    if row.is_file {
        spans.push(Span::styled("· ", dim));
    } else if row.has_children {
        if row.is_expanded {
            spans.push(Span::styled("▼ ", Style::default().fg(Color::Yellow)));
        } else {
            spans.push(Span::styled("▶ ", Style::default().fg(Color::Cyan)));
        }
    } else {
        spans.push(Span::raw("  "));
    }
    prefix_len += 2;

    // Size + percentages
    let size_str = format_size(row.total_size);
    let pct_parent = if row.parent_size > 0 {
        row.total_size as f64 / row.parent_size as f64 * 100.0
    } else {
        100.0
    };
    let pct_root = if row.root_size > 0 {
        row.total_size as f64 / row.root_size as f64 * 100.0
    } else {
        100.0
    };

    let bar_width = 16;
    let filled = ((pct_parent / 100.0) * bar_width as f64).round() as usize;
    let filled = filled.min(bar_width);
    let empty = bar_width - filled;

    let pct_parent_str = format!("{:>5.1}%", pct_parent);
    let pct_root_str = format!("{:>5.1}%", pct_root);

    // suffix: " {:>9}  {bar:16}  {:>6}  {:>6}" = 1+9+2+16+2+6+2+6 = 44
    let suffix_len = 44;

    let name_width = width.saturating_sub(prefix_len + suffix_len + 1);
    let mut name = if row.is_file {
        row.name.clone()
    } else {
        format!("{}/", row.name)
    };
    if name.len() > name_width {
        name.truncate(name_width.saturating_sub(1));
        name.push('…');
    }

    let color = size_color(row.total_size);
    let name_style = if row.is_file {
        Style::default().fg(color)
    } else {
        Style::default().fg(color).bold()
    };

    spans.push(Span::styled(
        format!("{:<width$}", name, width = name_width),
        name_style,
    ));

    spans.push(Span::styled(
        format!(" {:>9}", size_str),
        Style::default().fg(color),
    ));

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        "█".repeat(filled),
        Style::default().fg(bar_color(pct_parent)),
    ));
    spans.push(Span::styled("░".repeat(empty), dim));

    spans.push(Span::styled(
        format!(" {}", pct_parent_str),
        Style::default().fg(color),
    ));

    let root_color = if row.depth == 0 {
        Color::DarkGray
    } else {
        Color::Rgb(140, 140, 80)
    };
    spans.push(Span::styled(
        format!("  {}", pct_root_str),
        Style::default().fg(root_color),
    ));

    Line::from(spans)
}
