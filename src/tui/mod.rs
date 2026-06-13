mod colors;
mod input;
mod render;
mod row;
mod state;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;

use crate::model::DirNode;
use state::App;

// ── Entry point ─────────────────────────────────────────────────────────────

pub fn run(root: DirNode) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, root);

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, root: DirNode) -> Result<()> {
    let mut app = App::new(root);

    loop {
        terminal.draw(|frame| app.render(frame))?;

        app.check_pending_delete();

        let poll_timeout = if app.pending_delete.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(60)
        };

        if event::poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code, key.modifiers);
                    if app.should_quit {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
