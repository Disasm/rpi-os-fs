use std::cmp::min;
use std::io::{self, SeekFrom};

use vfat::{VFatFileSystem, Shared};
use vfat::cluster_chain::ClusterChain;
use traits::File;

pub struct VFatFile {
    chain: ClusterChain,
    size: u32,
    dir_start_cluster: u32,
    regular_entry_index: u64,
}

impl VFatFile {
    pub fn open(vfat: Shared<VFatFileSystem>, start_cluster: u32, size: u32) -> VFatFile {
        VFatFile {
            chain: ClusterChain::open(vfat, start_cluster),
            size,
            dir_start_cluster: unimplemented!(),
            regular_entry_index: unimplemented!(),
        }
    }

    pub fn at_end(&self) -> bool {
        self.chain.position == self.size as u64
    }
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
        self.chain.flush()
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
