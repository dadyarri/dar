use crate::pager::PagerWriter;
use eyre::Result;
use std::io::Write;

#[derive(Debug, Clone, Copy)]
pub enum ValidationLevel {
    /// Quick check: header, footer, overall checksum
    #[allow(dead_code)]
    Basic,
    /// Detailed check: all of basic + index parsing
    Full,
    /// Slow check: all of full + individual entry checksums
    Slow,
}

/// Validation context for tracking results
pub struct ValidationContext {
    verbose: bool,
    _file_size: u64,
    checks_passed: u32,
    checks_failed: u32,
    pub errors: Vec<String>,
    output: Option<PagerWriter>,
}

impl ValidationContext {
    pub fn new(file_size: u64, verbose: bool, output: PagerWriter) -> Self {
        Self {
            verbose,
            _file_size: file_size,
            checks_passed: 0,
            checks_failed: 0,
            errors: Vec::new(),
            output: Some(output),
        }
    }

    pub fn check(&mut self, name: &str, result: Result<()>) {
        match result {
            Ok(()) => {
                self.checks_passed += 1;
                if self.verbose {
                    if let Some(ref mut out) = self.output {
                        let _ = writeln!(out, "  ✓ {}", name);
                    }
                }
            }
            Err(e) => {
                self.checks_failed += 1;
                let msg = format!("{}: {}", name, e);
                self.errors.push(msg.clone());
                if self.verbose {
                    if let Some(ref mut out) = self.output {
                        let _ = writeln!(out, "  ✗ {}", msg);
                    }
                }
            }
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{} passed, {} failed",
            self.checks_passed, self.checks_failed
        )
    }

    pub fn is_valid(&self) -> bool {
        self.checks_failed == 0
    }

    pub fn writeln(&mut self, args: std::fmt::Arguments) -> std::io::Result<()> {
        if let Some(ref mut out) = self.output {
            writeln!(out, "{}", args)
        } else {
            Ok(())
        }
    }
}
