use std::fs::{self, DirEntry};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use rayon::prelude::*;
use regex::RegexSet;

use crate::model::{DirNode, FileEntry};

const FILE_STORE_LIMIT: usize = 200;
/// Directories with more files than this stat them in parallel chunks.
const PAR_STAT_THRESHOLD: usize = 2048;
const STAT_CHUNK: usize = 1024;

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

/// Exclude rules. Patterns containing `/` must match an entry's whole absolute
/// path (e.g. `/mnt`); all others match anywhere in its name.
pub struct Exclude {
    names: RegexSet,
    paths: RegexSet,
}

impl Exclude {
    pub fn new(patterns: &[String]) -> Result<Self, regex::Error> {
        let (paths, names): (Vec<&String>, Vec<&String>) =
            patterns.iter().partition(|p| p.contains('/'));
        Ok(Self {
            names: RegexSet::new(names)?,
            paths: RegexSet::new(paths.iter().map(|p| format!("^(?:{p})$")))?,
        })
    }

    fn is_match(&self, entry: &DirEntry) -> bool {
        if !self.names.is_empty() && self.names.is_match(&entry.file_name().to_string_lossy()) {
            return true;
        }
        if !self.paths.is_empty() {
            let path = entry.path();
            let path = path.to_string_lossy();
            #[cfg(windows)]
            let path = path.replace('\\', "/");
            return self.paths.is_match(&path);
        }
        false
    }
}

pub fn scan(path: &Path, exclude: &Exclude, progress: &ScanProgress) -> DirNode {
    scan_recursive(path, exclude, progress)
}

fn scan_recursive(path: &Path, exclude: &Exclude, progress: &ScanProgress) -> DirNode {
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

    let mut subdirs: Vec<PathBuf> = Vec::new();
    let mut file_entries: Vec<DirEntry> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // Classify via file_type(), which comes from readdir's d_type on most
    // filesystems, so only regular files pay for a stat call.
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        if exclude.is_match(&entry) {
            continue;
        }
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => subdirs.push(entry.path()),
            Ok(ft) if ft.is_file() => file_entries.push(entry),
            Ok(_) => {}
            Err(e) => errors.push(format!("{}: {}", entry.path().display(), e)),
        }
    }

    // Stat files and recurse into subdirectories concurrently. Huge flat
    // directories are split into chunks so their stats spread across cores.
    let (stats, mut children) = rayon::join(
        || {
            if file_entries.len() > PAR_STAT_THRESHOLD {
                file_entries
                    .par_chunks(STAT_CHUNK)
                    .map(|chunk| stat_files(chunk, progress))
                    .reduce(FileStats::default, FileStats::merge)
            } else {
                stat_files(&file_entries, progress)
            }
        },
        || {
            subdirs
                .par_iter()
                .map(|p| scan_recursive(p, exclude, progress))
                .collect::<Vec<DirNode>>()
        },
    );
    drop(file_entries);

    let FileStats {
        size: own_size,
        count: file_count,
        mut files,
        errors: stat_errors,
    } = stats;
    errors.extend(stat_errors);
    files.sort_unstable_by(|a, b| b.size.cmp(&a.size));

    children.sort_unstable_by(|a, b| b.total_size.cmp(&a.total_size));

    let children_size: u64 = children.iter().map(|c| c.total_size).sum();
    let total_size = own_size + children_size;
    let dir_count = children.len() as u64 + children.iter().map(|c| c.dir_count).sum::<u64>();
    let total_file_count = file_count + children.iter().map(|c| c.file_count).sum::<u64>();

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

#[derive(Default)]
struct FileStats {
    size: u64,
    count: u64,
    /// Largest files seen, at most FILE_STORE_LIMIT, unsorted.
    files: Vec<FileEntry>,
    errors: Vec<String>,
}

impl FileStats {
    fn merge(mut self, other: FileStats) -> FileStats {
        self.size += other.size;
        self.count += other.count;
        self.files.extend(other.files);
        keep_largest(&mut self.files, |f| f.size);
        self.errors.extend(other.errors);
        self
    }
}

fn stat_files(entries: &[DirEntry], progress: &ScanProgress) -> FileStats {
    let mut size: u64 = 0;
    let mut sized: Vec<(u64, &DirEntry)> = Vec::with_capacity(entries.len());
    let mut errors: Vec<String> = Vec::new();

    for entry in entries {
        match entry.metadata() {
            Ok(meta) => {
                size += meta.len();
                sized.push((meta.len(), entry));
            }
            Err(e) => errors.push(format!("{}: {}", entry.path().display(), e)),
        }
    }

    let count = sized.len() as u64;
    progress.files.fetch_add(count, Ordering::Relaxed);

    // Only allocate names for the files we actually keep.
    keep_largest(&mut sized, |&(size, _)| size);
    let files = sized
        .into_iter()
        .map(|(size, entry)| FileEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            size,
        })
        .collect();

    FileStats {
        size,
        count,
        files,
        errors,
    }
}

/// Truncate `items` to the FILE_STORE_LIMIT largest by `key`, in O(n).
fn keep_largest<T>(items: &mut Vec<T>, key: impl Fn(&T) -> u64) {
    if items.len() > FILE_STORE_LIMIT {
        items.select_nth_unstable_by(FILE_STORE_LIMIT - 1, |a, b| key(b).cmp(&key(a)));
        items.truncate(FILE_STORE_LIMIT);
    }
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}
