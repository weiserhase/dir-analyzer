# dir-analyzer

A blazingly fast directory size analyzer for the terminal, written in Rust. Think [TreeSize](https://www.jam-software.com/treesize_free) for Linux/macOS/WSL — with an interactive TUI and the ability to delete files and directories on the spot.

Scans **113,000+ directories and 770,000+ files in under 0.2 seconds** by parallelizing across all CPU cores.

## Features

- **Parallel scanning** — uses all available cores via [rayon](https://github.com/rayon-rs/rayon) work-stealing
- **Two modes** — static terminal report or interactive TUI tree explorer
- **Files and directories** — everything shown in one size-sorted view
- **Delete from the TUI** — `dd` then `y` to permanently remove files or directories (vim-style)
- **Depth-aware UI** — color-coded depth tags (`L0`, `L1`, `L2`…), per-depth tree line colors, alternating row backgrounds
- **Dual percentages** — % of parent and % of root shown side by side
- **Size-based coloring** — red (>1 GB), yellow (>100 MB), green (>10 MB), cyan (>1 MB)

## Demo

**Static report** (`dir-analyzer ~/projects -d 2`):

```
 /home/user/projects (4.2 GB, 12.3K files, 1.1K dirs)

 ├─ node_modules/              2.1 GB  ██████████░░░░░░░░░░   50.0%
 │   ├─ .cache/                  800 MB  ████████░░░░░░░░░░░░   38.1%
 │   ├─ typescript/              200 MB  ██░░░░░░░░░░░░░░░░░░    9.5%
 │   └─ ...
 ├─ target/                    1.5 GB  ███████░░░░░░░░░░░░░   35.7%
 │   └─ release/                 1.5 GB  ████████████████████  100.0%
 ├─ data.csv                   400 MB  ██░░░░░░░░░░░░░░░░░░    9.5%
 └─ src/                       100 MB  ░░░░░░░░░░░░░░░░░░░░    2.4%
     ├─ main.rs                    60 KB  ████████████░░░░░░░░   58.5%
     └─ lib.rs                     40 KB  ████████░░░░░░░░░░░░   39.0%
```

**Interactive TUI** (`dir-analyzer ~/projects -i`):

```
  Dir Analyzer  │  /home/user/projects  │  4.2 GB  │  12.3K files  │  1.1K dirs
────────────────────────────────────────────────────────────────────────────────
  Lvl  Tree / Name                  Size  % of Parent          % of Root
 L0  ▼ projects/              4.2 GB  ████████████████ 100.0%  100.0%
 L1   ├─ ▼ node_modules/      2.1 GB  ████████░░░░░░░░  50.0%   50.0%
 L2   │    ├─ ▶ .cache/       800 MB  ██████░░░░░░░░░░  38.1%   19.0%
 L2   │    └─ ▶ typescript/   200 MB  ██░░░░░░░░░░░░░░   9.5%    4.8%
 L1   ├─ ▶ target/            1.5 GB  ██████░░░░░░░░░░  35.7%   35.7%
 L1   ├─ · data.csv           400 MB  ███░░░░░░░░░░░░░   9.5%    9.5%
 L1   └─ ▶ src/               100 MB  █░░░░░░░░░░░░░░░   2.4%    2.4%
─────────────────────────────────────────────────────────────────────────
 L1  dir  → /home/user/projects/node_modules  │  2.1 GB  │  50.0% parent  │  50.0% root
 ↑↓/jk Nav  ←→/hl Expand  Enter Toggle  e Expand All  c Collapse All  dd Delete  q Quit
```

## Installation

### From source (recommended)

Requires [Rust](https://www.rust-lang.org/tools/install) 1.70+:

```bash
git clone https://github.com/jkeller/dir-analyzer.git
cd dir-analyzer
cargo install --path .
```

The binary will be installed to `~/.cargo/bin/dir-analyzer`.

### Build without installing

```bash
git clone https://github.com/jkeller/dir-analyzer.git
cd dir-analyzer
cargo build --release
# binary at ./target/release/dir-analyzer
```

### From crates.io (once published)

```bash
cargo install dir-analyzer
```

## Usage

```
dir-analyzer [OPTIONS] [PATH]

Arguments:
  [PATH]  Directory to analyze [default: .]

Options:
  -i, --interactive        Launch interactive tree explorer (TUI mode)
  -d, --depth <DEPTH>      Max depth for static report [default: 3]
  -t, --threads <THREADS>  Number of threads (defaults to all available cores)
  -h, --help               Print help
  -V, --version            Print version
```

### Examples

```bash
# Analyze current directory, 3 levels deep (default)
dir-analyzer

# Analyze home directory, show only top level
dir-analyzer ~ -d 0

# Launch interactive explorer on /var
dir-analyzer /var -i

# Static report, 5 levels deep, using 4 threads
dir-analyzer /usr -d 5 -t 4
```

## TUI Keybindings

| Key              | Action                               |
| ---------------- | ------------------------------------ |
| `↑` / `k`       | Move cursor up                       |
| `↓` / `j`       | Move cursor down                     |
| `→` / `l`       | Expand directory                     |
| `←` / `h`       | Collapse directory / go to parent    |
| `Enter` / `Space`| Toggle expand/collapse              |
| `e`              | Expand all children recursively      |
| `c`              | Collapse all children recursively    |
| `dd`             | Delete selected file/directory       |
| `y`              | Confirm deletion                     |
| `PgUp` / `PgDn` | Scroll 20 rows                       |
| `g` / `G`        | Jump to top / bottom                |
| `q` / `Esc`     | Quit                                 |

### Delete workflow

1. Navigate to the file or directory you want to remove
2. Press `d` once — a hint appears: *"Press d again to delete selected"*
3. Press `d` again — a confirmation dialog pops up showing the path and size
4. Press `y` to permanently delete, or **any other key** to cancel

**Warning:** Deletion is permanent (`rm -rf` for directories, `rm` for files). There is no undo.

## How it works

### Scanning

The scanner walks the filesystem using `std::fs::read_dir` and parallelizes subdirectory traversal with [rayon](https://github.com/rayon-rs/rayon). Each directory's children are scanned in parallel via rayon's work-stealing thread pool, which automatically scales to all available CPU cores. A real-time progress counter updates every 50ms during the scan.

### Data model

```
DirNode
├── name, path
├── own_size          (sum of files directly in this directory)
├── total_size        (own_size + all descendants)
├── children: Vec<DirNode>   (subdirectories, sorted by total_size desc)
├── files: Vec<FileEntry>    (files, sorted by size desc)
├── file_count, dir_count    (recursive counts)
└── errors            (permission denied, etc.)
```

When displaying, files and directories are merged into a single size-sorted list via an efficient merge of the two pre-sorted vectors.

### Rendering

- **Static mode:** Colored tree output to stdout using [crossterm](https://github.com/crossterm-rs/crossterm) ANSI styling
- **TUI mode:** Full-screen interactive UI built with [ratatui](https://github.com/ratatui/ratatui), using a flattened visible-row model computed from the tree + expanded-node set

## Project structure

```
src/
├── main.rs       CLI entry point, arg parsing, progress display
├── scanner.rs    Parallel directory scanner (rayon)
├── model.rs      DirNode, FileEntry, TreeEntry, formatting utilities
├── tui.rs        Interactive tree explorer (ratatui + crossterm)
└── report.rs     Static colored tree report
```

## Performance

Benchmarked on a typical Linux home directory:

| Metric | Value |
| ------ | ----- |
| Directories scanned | 113,000+ |
| Files scanned | 770,000+ |
| Total size | 127 GB |
| Scan time (release) | **~0.15s** |

The release build uses LTO and max optimization (`opt-level = 3`) for best performance.

## Requirements

- **Rust** 1.70+ (for building)
- **Linux**, **macOS**, or **WSL** (Windows Subsystem for Linux)
- A terminal that supports ANSI colors and Unicode (virtually all modern terminals)

## License

[MIT](LICENSE)
