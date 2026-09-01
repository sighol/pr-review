use std::io;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Pipes `content` through `$PAGER` (falling back to `less -R -F`, which
/// renders ANSI colors and skips paging when the content fits on one screen).
/// Deliberately omits `-X`: it skips entering the terminal's alternate
/// screen, which is what most terminal emulators key off of to translate
/// mouse-wheel scrolling into arrow keys for the foreground app.
pub async fn page(content: &str) -> Result<()> {
    let pager_cmd = std::env::var("PAGER").unwrap_or_else(|_| "less -R -F".to_string());
    let mut parts = pager_cmd.split_whitespace();
    let program = parts.next().unwrap_or("less");
    let args: Vec<&str> = parts.collect();

    let mut child = Command::new(program)
        .args(&args)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to launch pager '{pager_cmd}'"))?;

    if let Some(mut stdin) = child.stdin.take() {
        match stdin.write_all(content.as_bytes()).await {
            Ok(()) => {}
            // The pager (e.g. `less -F`) may exit as soon as it has enough to
            // fill the screen, closing its end of the pipe before we're done.
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {}
            Err(e) => return Err(e).context("failed to write diff to pager"),
        }
    }

    child.wait().await.context("pager exited with an error")?;
    Ok(())
}
