use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use rayon::prelude::*;

use crate::model::{DirNode, FileEntry};

#[derive(Clone)]
pub struct ScanProgress {
    dirs: Arc<AtomicU64>,
    files: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self {
            dirs: Arc::new(AtomicU64::new(0)),
            files: Arc::new(AtomicU64::new(0)),
            done: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn dirs_scanned(&self) -> u64 {
        self.dirs.load(Ordering::Relaxed)
    }

    pub fn files_scanned(&self) -> u64 {
        self.files.load(Ordering::Relaxed)
    }

    pub fn mark_done(&self) {
        self.done.store(true, Ordering::Relaxed);
    }

    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }
}

pub fn scan(path: &Path, progress: &ScanProgress) -> DirNode {
    scan_recursive(path, progress)
}

fn scan_recursive(path: &Path, progress: &ScanProgress) -> DirNode {
    progress.dirs.fetch_add(1, Ordering::Relaxed);

    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => {
            return DirNode {
                name: dir_name(path),
                path: path.to_path_buf(),
                own_size: 0,
                total_size: 0,
                children: Vec::new(),
                files: Vec::new(),
                own_file_count: 0,
                file_count: 0,
                dir_count: 0,
                errors: vec![e.to_string()],
            };
        }
    };

    let mut own_size: u64 = 0;
    let mut file_count: u64 = 0;
    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();
    let mut files: Vec<FileEntry> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for entry in entries {
        match entry {
            Ok(entry) => match entry.metadata() {
                Ok(meta) => {
                    if meta.is_dir() {
                        subdirs.push(entry.path());
                    } else if meta.is_file() {
                        let size = meta.len();
                        own_size += size;
                        file_count += 1;
                        files.push(FileEntry {
                            name: entry.file_name().to_string_lossy().into_owned(),
                            size,
                        });
                        progress.files.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    errors.push(format!("{}: {}", entry.path().display(), e));
                }
            },
            Err(e) => errors.push(e.to_string()),
        }
    }

    files.sort_unstable_by(|a, b| b.size.cmp(&a.size));

    let mut children: Vec<DirNode> = subdirs
        .par_iter()
        .map(|p| scan_recursive(p, progress))
        .collect();

    children.sort_unstable_by(|a, b| b.total_size.cmp(&a.total_size));

    let children_size: u64 = children.iter().map(|c| c.total_size).sum();
    let total_size = own_size + children_size;
    let dir_count =
        children.len() as u64 + children.iter().map(|c| c.dir_count).sum::<u64>();
    let total_file_count =
        file_count + children.iter().map(|c| c.file_count).sum::<u64>();

    DirNode {
        name: dir_name(path),
        path: path.to_path_buf(),
        own_size,
        total_size,
        children,
        files,
        own_file_count: file_count,
        file_count: total_file_count,
        dir_count,
        errors,
    }
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}
