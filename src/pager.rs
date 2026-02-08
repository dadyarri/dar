use std::io::{self, Write, IsTerminal};
use std::process::{Command, Stdio};

/// A writer that automatically pages output if connected to a terminal
pub struct PagerWriter {
    inner: Box<dyn Write>,
}

impl PagerWriter {
    /// Create a new pager writer that will page output if connected to a terminal
    pub fn new() -> io::Result<Self> {
        let writer: Box<dyn Write> = if io::stdout().is_terminal() {
            // Try to use the pager
            match create_pager() {
                Ok(pager) => Box::new(pager),
                Err(_) => {
                    // Fall back to stdout if pager fails
                    Box::new(io::stdout())
                }
            }
        } else {
            // Not a terminal, write directly to stdout
            Box::new(io::stdout())
        };

        Ok(PagerWriter { inner: writer })
    }
}

impl Write for PagerWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.write(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                // Silently ignore broken pipe errors (normal when pager closes)
                Ok(buf.len())
            }
            Err(e) => Err(e),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.inner.flush() {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                // Silently ignore broken pipe errors
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

fn create_pager() -> io::Result<impl Write> {
    let pager = std::env::var("PAGER").unwrap_or_else(|_| "less -R".to_string());
    let mut parts = pager.split_whitespace();
    let cmd = parts.next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::Other, "Invalid PAGER")
    })?;

    let child = Command::new(cmd)
        .args(parts)
        .stdin(Stdio::piped())
        .spawn()?;

    child.stdin.ok_or_else(|| {
        io::Error::new(io::ErrorKind::Other, "Failed to get pager stdin")
    })
}
