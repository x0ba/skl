//! First-run after `install.sh`: optional login + init (harness checklist).

use std::io::{self, BufRead, IsTerminal, Write};

use crate::commands::{init, login};
use crate::error::Result;
use crate::prompt;

/// Skip prompts when install used `--non-interactive`, CI, or a non-TTY.
pub fn is_interactive_setup() -> bool {
    if std::env::var_os("CI").is_some()
        || std::env::var_os("SKL_NO_PROMPT").is_some()
        || std::env::var_os("SKL_YES").is_some()
    {
        return false;
    }
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

pub async fn run(api_base: String, non_interactive: bool) -> Result<()> {
    if non_interactive || !is_interactive_setup() {
        eprintln!("Skipping first-run prompts (non-interactive).");
        eprintln!("Binary is installed. Next: skl login && skl init");
        return Ok(());
    }

    eprintln!("skl first-run");
    let mut stdin = io::BufReader::new(io::stdin());
    let mut stderr = io::stderr();
    first_run(&api_base, &mut stdin, &mut stderr).await
}

async fn first_run<R: BufRead, W: Write>(
    api_base: &str,
    reader: &mut R,
    writer: &mut W,
) -> Result<()> {
    if prompt::confirm_yes_default(reader, writer, "Log in now?")? {
        login::run(api_base.to_string(), None).await?;
    }
    if prompt::confirm_yes_default(reader, writer, "Import existing skills (skl init)?")? {
        // Harness checklist lives in init (Universal .agents locked).
        init::run(api_base.to_string()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn interactive_respects_no_prompt_env() {
        let prev = std::env::var_os("SKL_NO_PROMPT");
        std::env::set_var("SKL_NO_PROMPT", "1");
        assert!(!is_interactive_setup());
        match prev {
            Some(value) => std::env::set_var("SKL_NO_PROMPT", value),
            None => std::env::remove_var("SKL_NO_PROMPT"),
        }
    }

    #[test]
    fn confirm_lines_are_login_then_init() {
        let mut input = Cursor::new("n\nn\n");
        let mut out = Vec::new();
        let login = prompt::confirm_yes_default(&mut input, &mut out, "Log in now?").unwrap();
        let init = prompt::confirm_yes_default(
            &mut input,
            &mut out,
            "Import existing skills (skl init)?",
        )
        .unwrap();
        assert!(!login);
        assert!(!init);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Log in now? [Y/n]"));
        assert!(text.contains("Import existing skills (skl init)? [Y/n]"));
    }
}
