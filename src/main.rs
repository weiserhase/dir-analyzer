mod model;
mod report;
mod scanner;
mod tui;

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, Result};
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "dir-analyzer",
    version,
    about = "Fast parallel directory size analyzer with interactive tree explorer"
)]
struct Cli {
    /// Directory to analyze
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Launch interactive tree explorer (TUI mode)
    #[arg(short, long)]
    interactive: bool,

    /// Max depth for static report
    #[arg(short, long, default_value = "3")]
    depth: usize,

    /// Number of threads (defaults to all available cores)
    #[arg(short = 't', long)]
    threads: Option<usize>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let path = cli.path.canonicalize().unwrap_or_else(|_| cli.path.clone());
    if !path.is_dir() {
        bail!("{} is not a directory", path.display());
    }

    if let Some(n) = cli.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .ok();
    }

    let start = Instant::now();

    let progress = scanner::ScanProgress::new();
    let progress_display = progress.clone();

    let progress_thread = std::thread::spawn(move || {
        while !progress_display.is_done() {
            eprint!(
                "\r\x1b[K  Scanning... {} dirs, {} files",
                progress_display.dirs_scanned(),
                progress_display.files_scanned()
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    let root = scanner::scan(&path, &progress);
    progress.mark_done();
    progress_thread.join().ok();

    let elapsed = start.elapsed();
    eprintln!(
        "\r\x1b[K  Scanned {} dirs, {} files in {:.2}s\n",
        root.dir_count + 1,
        root.file_count,
        elapsed.as_secs_f64()
    );

    if cli.interactive {
        tui::run(root)?;
    } else {
        report::print_tree(&root, cli.depth);
    }

    Ok(())
}
