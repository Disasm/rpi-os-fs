use std::cmp::min;
use std::io::{self, Write, SeekFrom};

use vfat::cluster_chain::ClusterChain;
use traits::File;
use vfat::VFatEntry;
use traits::FileOpenMode;
use vfat::lock_manager::LockMode;
use traits::BlockDevice;

pub struct VFatFile {
    chain: ClusterChain,
    size: u32,
    old_size: u32,
    entry: VFatEntry,
}

impl Drop for VFatFile {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

impl VFatFile {
    pub fn from_entry(entry: &VFatEntry, mode: FileOpenMode) -> io::Result<VFatFile> {
        let vfat = entry.vfat();
        let mode = match mode {
            FileOpenMode::Read => LockMode::Read,
            FileOpenMode::Write => LockMode::Write,
        };
        let chain = ClusterChain::open(vfat, entry.metadata.first_cluster, mode)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "can't lock file"))?;

        let size = entry.current_file_size()?;
        Ok(VFatFile {
            chain,
            size,
            old_size: size,
            entry: entry.clone(),
        })
    }

    pub fn at_end(&self) -> bool {
        self.chain.position == self.size as u64
    }

    pub fn close(self) {}
}

impl io::Read for VFatFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at_end() {
            return Ok(0);
        }
        let read_size = min(buf.len() as u64, self.size as u64 - self.chain.position);
        self.chain.read(&mut buf[..read_size as usize])
    }
}

impl io::Write for VFatFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let write_size = self.chain.write(buf)?;

        if self.chain.position > self.size as u64 {
            if self.chain.position > ::std::u32::MAX as u64 {
                return Err(io::Error::new(io::ErrorKind::Other, "File is too fat for FAT32"));
            }
            self.size = self.chain.position as u32;
        }
        Ok(write_size)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.chain.flush()?;
        if self.size != self.old_size {
            self.entry.set_file_size(self.size)?;
            self.old_size = self.size;
        }
        self.chain.vfat.borrow_mut().device.sync()?;
        Ok(())
    }
}

impl File for VFatFile {
    fn size(&self) -> u64 {
        self.size as u64
    }
}

impl io::Seek for VFatFile {
    /// Seek to offset `pos` in the file.
    ///
    /// A seek to the end of the file is allowed. A seek _beyond_ the end of the
    /// file returns an `InvalidInput` error.
    ///
    /// If the seek operation completes successfully, this method returns the
    /// new position from the start of the stream. That position can be used
    /// later with SeekFrom::Start.
    ///
    /// # Errors
    ///
    /// Seeking before the start of a file or beyond the end of the file results
    /// in an `InvalidInput` error.
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(p) => {
                if p > ::std::u32::MAX as u64 {
                    return Err(io::Error::from(io::ErrorKind::InvalidInput));
                }
                p as i64
            }
            SeekFrom::End(p) => self.size as i64 - p,
            SeekFrom::Current(p) => self.chain.position as i64 + p,
        };
        if new_pos < 0 || new_pos > self.size as i64 {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        self.chain.seek(SeekFrom::Start(new_pos as u64))
    }
}
