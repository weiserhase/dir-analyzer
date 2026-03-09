use std::path::{Path, PathBuf};

pub struct FileEntry {
    pub name: String,
    pub size: u64,
}

pub struct DirNode {
    pub name: String,
    pub path: PathBuf,
    pub own_size: u64,
    pub total_size: u64,
    pub children: Vec<DirNode>,
    pub files: Vec<FileEntry>,
    pub own_file_count: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub errors: Vec<String>,
}

pub enum TreeEntry<'a> {
    Dir(&'a DirNode),
    File(&'a FileEntry, PathBuf),
}


impl DirNode {
    /// Merge children (dirs) and files into a single size-descending list.
    /// Both `children` and `files` are already sorted desc by the scanner.
    pub fn merged_entries(&self) -> Vec<TreeEntry<'_>> {
        let mut result = Vec::with_capacity(self.children.len() + self.files.len());
        let (mut di, mut fi) = (0, 0);
        while di < self.children.len() && fi < self.files.len() {
            if self.children[di].total_size >= self.files[fi].size {
                result.push(TreeEntry::Dir(&self.children[di]));
                di += 1;
            } else {
                let p = self.path.join(&self.files[fi].name);
                result.push(TreeEntry::File(&self.files[fi], p));
                fi += 1;
            }
        }
        while di < self.children.len() {
            result.push(TreeEntry::Dir(&self.children[di]));
            di += 1;
        }
        while fi < self.files.len() {
            let p = self.path.join(&self.files[fi].name);
            result.push(TreeEntry::File(&self.files[fi], p));
            fi += 1;
        }
        result
    }

    pub fn has_entries(&self) -> bool {
        !self.children.is_empty() || !self.files.is_empty()
    }

    pub fn find(&self, target: &Path) -> Option<&DirNode> {
        if self.path == target {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find(target) {
                return Some(found);
            }
        }
        None
    }

    pub fn remove_dir_at(&mut self, target: &Path) -> bool {
        if let Some(idx) = self.children.iter().position(|c| c.path == target) {
            self.children.remove(idx);
            self.recalculate();
            return true;
        }
        for child in &mut self.children {
            if child.remove_dir_at(target) {
                self.recalculate();
                return true;
            }
        }
        false
    }

    pub fn remove_file_at(&mut self, target: &Path) -> bool {
        if let Some(idx) = self
            .files
            .iter()
            .position(|f| self.path.join(&f.name) == target)
        {
            let file = self.files.remove(idx);
            self.own_size = self.own_size.saturating_sub(file.size);
            self.own_file_count = self.own_file_count.saturating_sub(1);
            self.recalculate();
            return true;
        }
        for child in &mut self.children {
            if child.remove_file_at(target) {
                self.recalculate();
                return true;
            }
        }
        false
    }

    fn recalculate(&mut self) {
        self.total_size =
            self.own_size + self.children.iter().map(|c| c.total_size).sum::<u64>();
        self.file_count =
            self.own_file_count + self.children.iter().map(|c| c.file_count).sum::<u64>();
        self.dir_count = self.children.len() as u64
            + self.children.iter().map(|c| c.dir_count).sum::<u64>();
    }
}

pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.1} TB", b / TB)
    } else if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}
