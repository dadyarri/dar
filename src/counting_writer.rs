use std::io::{Error, Write};

pub struct CountingWriter<W: Write> {
    inner: W,
    pub bytes_written: u64,
}

impl<W: Write> CountingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            bytes_written: 0,
        }
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let n = self.inner.write(buf)?;
        self.bytes_written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.inner.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CountingWriter;
    use std::io::Write;

    #[test]
    fn test_bytes_written_starts_at_zero() {
        let writer = CountingWriter::new(Vec::<u8>::new());
        assert_eq!(writer.bytes_written, 0);
    }

    #[test]
    fn test_writing_five_bytes_increments_counter() {
        let mut writer = CountingWriter::new(Vec::<u8>::new());
        writer.write_all(b"hello").unwrap();
        assert_eq!(writer.bytes_written, 5);
    }

    #[test]
    fn test_multiple_writes_accumulate() {
        let mut writer = CountingWriter::new(Vec::<u8>::new());
        writer.write_all(b"foo").unwrap();
        writer.write_all(b"bar").unwrap();
        assert_eq!(writer.bytes_written, 6);
    }

    #[test]
    fn test_flush_succeeds() {
        let mut writer = CountingWriter::new(Vec::<u8>::new());
        writer.write_all(b"data").unwrap();
        assert!(writer.flush().is_ok());
        assert_eq!(writer.bytes_written, 4);
    }
}
