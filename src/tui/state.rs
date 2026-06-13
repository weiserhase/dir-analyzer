use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use anyhow::Result;
use ratatui::style::Style;

use crate::model::{format_count, format_size, DirNode, TreeEntry};

use super::row::VisibleRow;

const FILE_DISPLAY_LIMIT: usize = 10;

pub(super) enum DeleteState {
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

pub(super) struct StatusMessage {
    pub text: String,
    pub style: Style,
    pub created: Instant,
}

pub(super) struct PendingDelete {
    pub path: PathBuf,
    pub name: String,
    pub is_file: bool,
    pub receiver: mpsc::Receiver<Result<(), String>>,
}

pub(super) struct App {
    pub root: DirNode,
    pub expanded: HashSet<PathBuf>,
    pub show_all_files: HashSet<PathBuf>,
    pub cursor: usize,
    pub should_quit: bool,
    pub delete_state: DeleteState,
    pub status: Option<StatusMessage>,
    pub pending_delete: Option<PendingDelete>,
}

impl App {
    pub(super) fn new(root: DirNode) -> Self {
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

    pub(super) fn visible_rows(&self) -> Vec<VisibleRow> {
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

    pub(super) fn clamp_cursor(&mut self) {
        let len = self.visible_rows().len();
        if self.cursor >= len {
            self.cursor = len.saturating_sub(1);
        }
    }
}
