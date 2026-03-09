use crate::models::archive::{ArchiveFooter, ArchiveHeader};
use eyre::{Context, Result};
use std::io::{Seek, Write};

pub struct ArchiveBuilder<W: Write + Seek> {
    writer: W,
    index_offset: u32,
    amount_of_files: u32,
}

impl<W: Write + Seek> ArchiveBuilder<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            index_offset: 0,
            amount_of_files: 0,
        }
    }

    pub fn write_header(&mut self) -> Result<()> {
        ArchiveHeader::new()
            .write(&mut self.writer)
            .wrap_err("Failed to write archive header")?;

        self.index_offset += size_of::<ArchiveHeader>() as u32 + 1;

        Ok(())
    }

    pub fn build(&mut self) -> Result<()> {
        ArchiveFooter::new(self.index_offset, self.amount_of_files)
            .write(&mut self.writer)
            .wrap_err("Failed to write archive footer")?;

        self.writer.flush().wrap_err("Failed to flush archive")?;

        Ok(())
    }
}
