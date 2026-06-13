use std::sync::mpsc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::{Color, Style, Stylize};

use super::row::collect_descendant_paths;
use super::state::{App, DeleteState, PendingDelete, StatusMessage};

impl App {
    pub(super) fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
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

    pub(super) fn check_pending_delete(&mut self) {
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
}
