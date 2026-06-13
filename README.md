# dir-analyzer

A blazingly fast directory size analyzer for the terminal, written in Rust, with an interactive TUI and on-the-spot file/directory deletion. Scans **113,000+ dirs and 770,000+ files in under 0.2s** by parallelizing across all CPU cores.

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

## Install

Install to your PATH (`~/.cargo/bin/dir-analyzer`):

```bash
cargo install --path .
```

### Build a standalone binary

```bash
cargo build --release
# → target/release/dir-analyzer (or dir-analyzer.exe on Windows)
```

The release binary is optimized (LTO, `opt-level = 3`) and self-contained — copy it to any machine of the same OS and run it directly, no runtime needed.

Prebuilt binaries for Linux, macOS, and Windows are attached to each [GitHub release](https://github.com/jkeller/dir-analyzer/releases).

## Usage

```bash
dir-analyzer [PATH]        # static report (default depth 3)
dir-analyzer [PATH] -i     # interactive TUI explorer
```

Options: `-i` interactive, `-d <N>` max depth, `-t <N>` threads. Inside the TUI, `dd` then `y` deletes the selected file/directory (permanent, no undo).

## License

[MIT](LICENSE)
