use std::io::{self, Write};

use crossterm::style::Stylize;

use crate::model::{format_count, format_size, DirNode, TreeEntry};

const FILE_DISPLAY_LIMIT: usize = 10;

pub fn print_tree(root: &DirNode, max_depth: usize) {
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    writeln!(
        out,
        "{}",
        format!(
            " {} ({}, {} files, {} dirs)",
            root.path.display(),
            format_size(root.total_size),
            format_count(root.file_count),
            format_count(root.dir_count),
        )
        .bold()
    )
    .ok();
    writeln!(out).ok();

    print_children(&mut out, root, max_depth, 0, &[]);

    if !root.errors.is_empty() {
        writeln!(out).ok();
        writeln!(
            out,
            "{}",
            format!(" {} errors encountered", root.errors.len()).red()
        )
        .ok();
    }
}

fn print_children<W: Write>(
    out: &mut W,
    node: &DirNode,
    max_depth: usize,
    depth: usize,
    ancestor_is_last: &[bool],
) {
    let entries = node.merged_entries();

    let mut display: Vec<&TreeEntry> = Vec::new();
    let mut files_shown: usize = 0;
    let mut hidden_count: usize = 0;
    let mut hidden_size: u64 = 0;

    for entry in &entries {
        match entry {
            TreeEntry::Dir(_) => display.push(entry),
            TreeEntry::File(f, _) => {
                if files_shown < FILE_DISPLAY_LIMIT {
                    display.push(entry);
                    files_shown += 1;
                } else {
                    hidden_count += 1;
                    hidden_size += f.size;
                }
            }
        }
    }

    let has_cutoff = hidden_count > 0;

    for (i, entry) in display.iter().enumerate() {
        let is_last = !has_cutoff && i == display.len() - 1;
        match entry {
            TreeEntry::Dir(child) => {
                print_dir_node(
                    out,
                    child,
                    node.total_size,
                    max_depth,
                    depth,
                    ancestor_is_last,
                    is_last,
                );
            }
            TreeEntry::File(file, _path) => {
                print_file_node(
                    out,
                    &file.name,
                    file.size,
                    node.total_size,
                    ancestor_is_last,
                    is_last,
                );
            }
        }
    }

    if has_cutoff {
        let prefix = build_prefix(ancestor_is_last, true);
        writeln!(
            out,
            "{}",
            format!(
                "{}... ({} more files, {} total)",
                prefix,
                hidden_count,
                format_size(hidden_size)
            )
            .dark_grey()
        )
        .ok();
    }
}

fn print_dir_node<W: Write>(
    out: &mut W,
    node: &DirNode,
    parent_size: u64,
    max_depth: usize,
    depth: usize,
    ancestor_is_last: &[bool],
    is_last: bool,
) {
    let prefix = build_prefix(ancestor_is_last, is_last);

    let size_str = format_size(node.total_size);
    let pct = if parent_size > 0 {
        node.total_size as f64 / parent_size as f64 * 100.0
    } else {
        0.0
    };

    let bar_width = 20;
    let filled = ((pct / 100.0) * bar_width as f64).round().min(bar_width as f64) as usize;
    let empty = bar_width - filled;

    let name_display = format!("{}/", node.name);

    let line = format!(
        "{}{:<30} {:>9}  {}{}  {:>5.1}%",
        prefix,
        name_display,
        size_str,
        "█".repeat(filled),
        "░".repeat(empty),
        pct
    );

    let styled = apply_size_color(&line, node.total_size);
    writeln!(out, "{styled}").ok();

    if depth < max_depth {
        let mut new_ancestors = ancestor_is_last.to_vec();
        new_ancestors.push(is_last);
        print_children(out, node, max_depth, depth + 1, &new_ancestors);
    } else if node.has_entries() {
        let cont_prefix = build_continuation_prefix(ancestor_is_last, is_last);
        let count = node.dir_count as usize + node.files.len();
        writeln!(
            out,
            "{}",
            format!("{}  └─ ({} more entries...)", cont_prefix, count).dark_grey()
        )
        .ok();
    }
}

fn print_file_node<W: Write>(
    out: &mut W,
    name: &str,
    size: u64,
    parent_size: u64,
    ancestor_is_last: &[bool],
    is_last: bool,
) {
    let prefix = build_prefix(ancestor_is_last, is_last);

    let size_str = format_size(size);
    let pct = if parent_size > 0 {
        size as f64 / parent_size as f64 * 100.0
    } else {
        0.0
    };

    let bar_width = 20;
    let filled = ((pct / 100.0) * bar_width as f64).round().min(bar_width as f64) as usize;
    let empty = bar_width - filled;

    let line = format!(
        "{}{:<30} {:>9}  {}{}  {:>5.1}%",
        prefix,
        name,
        size_str,
        "█".repeat(filled),
        "░".repeat(empty),
        pct
    );

    let styled = apply_size_color(&line, size);
    writeln!(out, "{styled}").ok();
}

fn build_prefix(ancestor_is_last: &[bool], is_last: bool) -> String {
    let mut prefix = String::new();
    for &last in ancestor_is_last {
        if last {
            prefix.push_str("    ");
        } else {
            prefix.push_str(" │  ");
        }
    }
    if is_last {
        prefix.push_str(" └─ ");
    } else {
        prefix.push_str(" ├─ ");
    }
    prefix
}

fn build_continuation_prefix(ancestor_is_last: &[bool], is_last: bool) -> String {
    let mut prefix = String::new();
    for &last in ancestor_is_last {
        if last {
            prefix.push_str("    ");
        } else {
            prefix.push_str(" │  ");
        }
    }
    if is_last {
        prefix.push_str("    ");
    } else {
        prefix.push_str(" │  ");
    }
    prefix
}

fn apply_size_color(text: &str, size: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB100: u64 = 100 * 1024 * 1024;
    const MB10: u64 = 10 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if size >= GB {
        format!("{}", text.red())
    } else if size >= MB100 {
        format!("{}", text.yellow())
    } else if size >= MB10 {
        format!("{}", text.green())
    } else if size >= MB {
        format!("{}", text.cyan())
    } else {
        format!("{}", text.white())
    }
}
