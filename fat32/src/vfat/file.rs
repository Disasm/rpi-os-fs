use std::cmp::min;
use std::io::{self, SeekFrom};

use traits;
use vfat::{VFat, Shared};
use vfat::cluster_chain::ClusterChain;

pub struct File {
    chain: ClusterChain,
    size: u32,
}

impl File {
    pub fn open(vfat: Shared<VFat>, start_cluster: u32, size: u32) -> File {
        File {
            chain: ClusterChain::open(vfat, start_cluster),
            size,
        }
    }

    pub fn at_end(&self) -> bool {
        self.chain.position == self.size
    }
}

impl io::Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at_end() {
            return Ok(0);
        }
        let read_size = min(buf.len() as u32, self.size - self.chain.position);
        self.chain.read(&mut buf[..read_size as usize])
    }
}

impl io::Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        unimplemented!()
    }

    fn flush(&mut self) -> io::Result<()> {
        unimplemented!()
    }
}

impl traits::File for File {
    fn sync(&mut self) -> io::Result<()> {
        unimplemented!()
    }

    fn size(&self) -> u64 {
        self.size as u64
    }
}

impl io::Seek for File {
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
        unimplemented!("File::seek()")
    }
}
