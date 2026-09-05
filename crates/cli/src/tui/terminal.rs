//! Alternate-screen + raw-mode guard. Always restore, including on panic.

use std::io::{self, Stdout};

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::error::{Result, SklError};

/// RAII terminal: raw mode + alt screen. Drop restores even if we panic.
pub struct TuiTerminal {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    restored: bool,
}

impl TuiTerminal {
    pub fn enter() -> Result<Self> {
        enable_raw_mode().map_err(|err| {
            SklError::LocalState(format!(
                "cannot enable raw mode ({err}); use `skl --help` or a subcommand"
            ))
        })?;
        let mut stdout = io::stdout();
        if let Err(err) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(SklError::LocalState(format!(
                "cannot enter alternate screen ({err})"
            )));
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|err| {
            let _ = restore_now();
            SklError::LocalState(format!("cannot init TUI backend ({err})"))
        })?;
        Ok(Self {
            terminal,
            restored: false,
        })
    }

    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    /// Leave raw/alt so `$EDITOR` or a blocking `skl sync` can use the TTY.
    pub fn suspend(&mut self) -> Result<()> {
        self.restore()?;
        Ok(())
    }

    /// Re-enter after [`Self::suspend`].
    pub fn resume(&mut self) -> Result<()> {
        enable_raw_mode().map_err(|err| {
            SklError::LocalState(format!("cannot re-enable raw mode ({err})"))
        })?;
        execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;
        self.terminal.clear()?;
        self.restored = false;
        Ok(())
    }

    fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        restore_now()?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for TuiTerminal {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn restore_now() -> io::Result<()> {
    let _ = disable_raw_mode();
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
