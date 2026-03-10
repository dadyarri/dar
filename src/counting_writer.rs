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
