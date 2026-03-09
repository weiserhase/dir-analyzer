use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::model::{format_count, format_size, DirNode, TreeEntry};

const FILE_DISPLAY_LIMIT: usize = 10;

// ── Depth palette: distinct hue per nesting level ───────────────────────────

const DEPTH_COLORS: [Color; 8] = [
    Color::Rgb(100, 180, 255), // L0 - blue
    Color::Rgb(180, 140, 255), // L1 - purple
    Color::Rgb(255, 180, 100), // L2 - orange
    Color::Rgb(100, 220, 160), // L3 - teal
    Color::Rgb(255, 130, 130), // L4 - salmon
    Color::Rgb(220, 200, 100), // L5 - gold
    Color::Rgb(130, 200, 255), // L6 - sky
    Color::Rgb(200, 160, 200), // L7 - mauve
];

const DEPTH_BG: [Color; 2] = [Color::Rgb(20, 20, 30), Color::Rgb(28, 28, 38)];

fn depth_fg(depth: usize) -> Color {
    DEPTH_COLORS[depth % DEPTH_COLORS.len()]
}

fn depth_bg(depth: usize) -> Color {
    DEPTH_BG[depth % 2]
}

// ── Data types ──────────────────────────────────────────────────────────────

struct VisibleRow {
    path: PathBuf,
    name: String,
    total_size: u64,
    own_size: u64,
    is_file: bool,
    is_file_cutoff: bool,
    has_children: bool,
    is_expanded: bool,
    depth: usize,
    ancestor_is_last: Vec<bool>,
    is_last: bool,
    parent_size: u64,
    root_size: u64,
    file_count: u64,
    dir_count: u64,
}

enum DeleteState {
    Normal,
    PendingD,
    Confirm {
        path: PathBuf,
        name: String,
        size: u64,
        is_root: bool,
        is_file: bool,
    },
}

struct StatusMessage {
    text: String,
    style: Style,
    created: Instant,
}

struct PendingDelete {
    path: PathBuf,
    name: String,
    is_file: bool,
    receiver: mpsc::Receiver<Result<(), String>>,
}

struct App {
    root: DirNode,
    expanded: HashSet<PathBuf>,
    show_all_files: HashSet<PathBuf>,
    cursor: usize,
    should_quit: bool,
    delete_state: DeleteState,
    status: Option<StatusMessage>,
    pending_delete: Option<PendingDelete>,
}

impl App {
    fn new(root: DirNode) -> Self {
        let mut expanded = HashSet::new();
        expanded.insert(root.path.clone());
        Self {
            root,
            expanded,
            show_all_files: HashSet::new(),
            cursor: 0,
            should_quit: false,
            delete_state: DeleteState::Normal,
            status: None,
            pending_delete: None,
        }
    }

    fn visible_rows(&self) -> Vec<VisibleRow> {
        let mut rows = Vec::new();
        let root_size = self.root.total_size;
        self.collect_visible(&self.root, 0, &[], true, root_size, root_size, &mut rows);
        rows
    }

    fn collect_visible(
        &self,
        node: &DirNode,
        depth: usize,
        ancestor_is_last: &[bool],
        is_last: bool,
        parent_size: u64,
        root_size: u64,
        rows: &mut Vec<VisibleRow>,
    ) {
        let is_expanded = self.expanded.contains(&node.path);
        let has_children = node.has_entries();

        rows.push(VisibleRow {
            path: node.path.clone(),
            name: node.name.clone(),
            total_size: node.total_size,
            own_size: node.own_size,
            is_file: false,
            is_file_cutoff: false,
            has_children,
            is_expanded,
            depth,
            ancestor_is_last: ancestor_is_last.to_vec(),
            is_last,
            parent_size,
            root_size,
            file_count: node.file_count,
            dir_count: node.dir_count,
        });

        if !(is_expanded && has_children) {
            return;
        }

        let mut child_ancestors = ancestor_is_last.to_vec();
        child_ancestors.push(is_last);

        let entries = node.merged_entries();
        let show_all = self.show_all_files.contains(&node.path);

        let mut display: Vec<&TreeEntry> = Vec::new();
        let mut files_shown: usize = 0;
        let mut files_shown_size: u64 = 0;

        for entry in &entries {
            match entry {
                TreeEntry::Dir(_) => display.push(entry),
                TreeEntry::File(f, _) => {
                    if show_all || files_shown < FILE_DISPLAY_LIMIT {
                        display.push(entry);
                        files_shown += 1;
                        files_shown_size += f.size;
                    }
                }
            }
        }

        let total_hidden_count = (node.own_file_count as usize).saturating_sub(files_shown);
        let total_hidden_size = node.own_size.saturating_sub(files_shown_size);
        let has_cutoff = total_hidden_count > 0;
        let cutoff_expandable = !show_all && node.files.len() > files_shown;

        for (i, entry) in display.iter().enumerate() {
            let entry_is_last = !has_cutoff && i == display.len() - 1;
            match entry {
                TreeEntry::Dir(child) => {
                    self.collect_visible(
                        child,
                        depth + 1,
                        &child_ancestors,
                        entry_is_last,
                        node.total_size,
                        root_size,
                        rows,
                    );
                }
                TreeEntry::File(file, file_path) => {
                    rows.push(VisibleRow {
                        path: file_path.clone(),
                        name: file.name.clone(),
                        total_size: file.size,
                        own_size: file.size,
                        is_file: true,
                        is_file_cutoff: false,
                        has_children: false,
                        is_expanded: false,
                        depth: depth + 1,
                        ancestor_is_last: child_ancestors.clone(),
                        is_last: entry_is_last,
                        parent_size: node.total_size,
                        root_size,
                        file_count: 0,
                        dir_count: 0,
                    });
                }
            }
        }

        if has_cutoff {
            let label = if cutoff_expandable {
                format!(
                    "... ({} more files, {})",
                    format_count(total_hidden_count as u64),
                    format_size(total_hidden_size),
                )
            } else {
                format!(
                    "... ({} small files not tracked, {})",
                    format_count(total_hidden_count as u64),
                    format_size(total_hidden_size),
                )
            };
            rows.push(VisibleRow {
                path: node.path.clone(),
                name: label,
                total_size: total_hidden_size,
                own_size: 0,
                is_file: false,
                is_file_cutoff: true,
                has_children: false,
                is_expanded: false,
                depth: depth + 1,
                ancestor_is_last: child_ancestors,
                is_last: true,
                parent_size: node.total_size,
                root_size,
                file_count: 0,
                dir_count: 0,
            });
        }
    }

    // ── Key handling ────────────────────────────────────────────────────────

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if let Some(ref s) = self.status {
            if s.created.elapsed().as_secs() >= 3 {
                self.status = None;
            }
        }

        if let DeleteState::Confirm { .. } = &self.delete_state {
            self.handle_confirm_key(code);
            return;
        }

        if let DeleteState::PendingD = &self.delete_state {
            if code == KeyCode::Char('d') {
                self.initiate_delete();
                return;
            }
            self.delete_state = DeleteState::Normal;
        }

        let rows = self.visible_rows();
        let row_count = rows.len();
        if row_count == 0 {
            return;
        }

        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }

            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.cursor + 1 < row_count {
                    self.cursor += 1;
                }
            }

            KeyCode::Home | KeyCode::Char('g') => self.cursor = 0,
            KeyCode::End | KeyCode::Char('G') => {
                self.cursor = row_count.saturating_sub(1);
            }

            KeyCode::PageUp => self.cursor = self.cursor.saturating_sub(20),
            KeyCode::PageDown => {
                self.cursor = (self.cursor + 20).min(row_count.saturating_sub(1));
            }

            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(row) = rows.get(self.cursor) {
                    if row.is_file_cutoff {
                        if !self.show_all_files.contains(&row.path) {
                            self.show_all_files.insert(row.path.clone());
                        }
                    } else if !row.is_file && row.has_children && !row.is_expanded {
                        self.expanded.insert(row.path.clone());
                    }
                }
            }

            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(row) = rows.get(self.cursor) {
                    if row.is_file || row.is_file_cutoff {
                        for i in (0..self.cursor).rev() {
                            if rows[i].depth < row.depth {
                                self.cursor = i;
                                break;
                            }
                        }
                    } else if row.is_expanded {
                        self.expanded.remove(&row.path);
                        self.clamp_cursor();
                    } else if row.depth > 0 {
                        for i in (0..self.cursor).rev() {
                            if rows[i].depth < row.depth {
                                self.cursor = i;
                                break;
                            }
                        }
                    }
                }
            }

            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(row) = rows.get(self.cursor) {
                    if row.is_file_cutoff {
                        if !self.show_all_files.contains(&row.path) {
                            self.show_all_files.insert(row.path.clone());
                        }
                    } else if !row.is_file && row.has_children {
                        if row.is_expanded {
                            self.expanded.remove(&row.path);
                            self.show_all_files.remove(&row.path);
                            self.clamp_cursor();
                        } else {
                            self.expanded.insert(row.path.clone());
                        }
                    }
                }
            }

            KeyCode::Char('e') => {
                if let Some(row) = rows.get(self.cursor) {
                    if !row.is_file && !row.is_file_cutoff {
                        let path = row.path.clone();
                        let paths = collect_descendant_paths(&self.root, &path);
                        for p in paths {
                            self.expanded.insert(p);
                        }
                    }
                }
            }

            KeyCode::Char('c') => {
                if let Some(row) = rows.get(self.cursor) {
                    if !row.is_file && !row.is_file_cutoff {
                        let path = row.path.clone();
                        let paths = collect_descendant_paths(&self.root, &path);
                        for p in &paths {
                            self.expanded.remove(p);
                            self.show_all_files.remove(p);
                        }
                        self.clamp_cursor();
                    }
                }
            }

            KeyCode::Char('d') => {
                if self.pending_delete.is_none() {
                    if let Some(row) = rows.get(self.cursor) {
                        if !row.is_file_cutoff {
                            self.delete_state = DeleteState::PendingD;
                        }
                    }
                }
            }

            _ => {}
        }
    }

    fn initiate_delete(&mut self) {
        let rows = self.visible_rows();
        if let Some(row) = rows.get(self.cursor) {
            if row.is_file_cutoff {
                self.delete_state = DeleteState::Normal;
                return;
            }
            let is_root = row.depth == 0 && !row.is_file;
            self.delete_state = DeleteState::Confirm {
                path: row.path.clone(),
                name: row.name.clone(),
                size: row.total_size,
                is_root,
                is_file: row.is_file,
            };
        } else {
            self.delete_state = DeleteState::Normal;
        }
    }

    fn handle_confirm_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') => {
                let (path, name, is_root, is_file) = match &self.delete_state {
                    DeleteState::Confirm {
                        path,
                        name,
                        is_root,
                        is_file,
                        ..
                    } => (path.clone(), name.clone(), *is_root, *is_file),
                    _ => {
                        self.delete_state = DeleteState::Normal;
                        return;
                    }
                };

                self.delete_state = DeleteState::Normal;

                if is_root {
                    self.status = Some(StatusMessage {
                        text: "Cannot delete the root scan directory".into(),
                        style: Style::default().fg(Color::Red).bold(),
                        created: Instant::now(),
                    });
                    return;
                }

                let (tx, rx) = mpsc::channel();
                let delete_path = path.clone();
                std::thread::spawn(move || {
                    let result = if is_file {
                        std::fs::remove_file(&delete_path)
                    } else {
                        std::fs::remove_dir_all(&delete_path)
                    };
                    let _ = tx.send(result.map_err(|e| e.to_string()));
                });

                self.pending_delete = Some(PendingDelete {
                    path,
                    name,
                    is_file,
                    receiver: rx,
                });
                self.status = Some(StatusMessage {
                    text: "Deleting...".into(),
                    style: Style::default().fg(Color::Yellow).bold(),
                    created: Instant::now(),
                });
            }
            _ => {
                self.delete_state = DeleteState::Normal;
                self.status = Some(StatusMessage {
                    text: "Delete cancelled".into(),
                    style: Style::default().fg(Color::DarkGray),
                    created: Instant::now(),
                });
            }
        }
    }

    fn check_pending_delete(&mut self) {
        let result = if let Some(ref pending) = self.pending_delete {
            match pending.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => {
                    self.status = Some(StatusMessage {
                        text: "Deleting...".into(),
                        style: Style::default().fg(Color::Yellow).bold(),
                        created: Instant::now(),
                    });
                    None
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Delete thread crashed".to_string()))
                }
            }
        } else {
            return;
        };

        if let Some(result) = result {
            let pending = self.pending_delete.take().unwrap();
            match result {
                Ok(()) => {
                    if pending.is_file {
                        self.root.remove_file_at(&pending.path);
                    } else {
                        self.expanded.remove(&pending.path);
                        self.show_all_files.remove(&pending.path);
                        self.root.remove_dir_at(&pending.path);
                    }
                    self.clamp_cursor();
                    let label = if pending.is_file {
                        pending.name
                    } else {
                        format!("{}/", pending.name)
                    };
                    self.status = Some(StatusMessage {
                        text: format!("Deleted {}", label),
                        style: Style::default().fg(Color::Green).bold(),
                        created: Instant::now(),
                    });
                }
                Err(e) => {
                    self.status = Some(StatusMessage {
                        text: format!("Delete failed: {}", e),
                        style: Style::default().fg(Color::Red).bold(),
                        created: Instant::now(),
                    });
                }
            }
        }
    }

    fn clamp_cursor(&mut self) {
        let len = self.visible_rows().len();
        if self.cursor >= len {
            self.cursor = len.saturating_sub(1);
        }
    }

    // ── Rendering ───────────────────────────────────────────────────────────

    fn render(&self, frame: &mut Frame) {
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
                        Style::default().fg(if is_expandable { Color::Cyan } else { Color::DarkGray }),
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
                    spans.push(Span::styled(
                        "  │  ",
                        Style::default().fg(Color::DarkGray),
                    ));
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
                let line =
                    Line::from(vec![Span::styled(format!("  {}", msg.text), msg.style)]);
                let bar =
                    Paragraph::new(line).style(Style::default().bg(Color::Rgb(25, 25, 35)));
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
        spans.push(Span::styled(truncated, Style::default().fg(Color::Rgb(100, 100, 130))));
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

// ── Helpers ─────────────────────────────────────────────────────────────────

fn collect_descendant_paths(root: &DirNode, target: &PathBuf) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(node) = root.find(target) {
        collect_paths_recursive(node, &mut paths);
    }
    paths
}

fn collect_paths_recursive(node: &DirNode, paths: &mut Vec<PathBuf>) {
    paths.push(node.path.clone());
    for child in &node.children {
        collect_paths_recursive(child, paths);
    }
}

fn size_color(size: u64) -> Color {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB100: u64 = 100 * 1024 * 1024;
    const MB10: u64 = 10 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if size >= GB {
        Color::Red
    } else if size >= MB100 {
        Color::Yellow
    } else if size >= MB10 {
        Color::Green
    } else if size >= MB {
        Color::Cyan
    } else {
        Color::White
    }
}

fn bar_color(pct: f64) -> Color {
    if pct >= 75.0 {
        Color::Red
    } else if pct >= 50.0 {
        Color::Yellow
    } else if pct >= 25.0 {
        Color::Green
    } else {
        Color::Cyan
    }
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub fn run(root: DirNode) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, root);

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, root: DirNode) -> Result<()> {
    let mut app = App::new(root);

    loop {
        terminal.draw(|frame| app.render(frame))?;

        app.check_pending_delete();

        let poll_timeout = if app.pending_delete.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(60)
        };

        if event::poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code, key.modifiers);
                    if app.should_quit {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
