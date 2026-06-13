use std::path::PathBuf;

use crate::model::DirNode;

/// A single flattened, renderable line of the tree.
pub(super) struct VisibleRow {
    pub path: PathBuf,
    pub name: String,
    pub total_size: u64,
    pub own_size: u64,
    pub is_file: bool,
    pub is_file_cutoff: bool,
    pub has_children: bool,
    pub is_expanded: bool,
    pub depth: usize,
    pub ancestor_is_last: Vec<bool>,
    pub is_last: bool,
    pub parent_size: u64,
    pub root_size: u64,
    pub file_count: u64,
    pub dir_count: u64,
}

pub(super) fn collect_descendant_paths(root: &DirNode, target: &PathBuf) -> Vec<PathBuf> {
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
