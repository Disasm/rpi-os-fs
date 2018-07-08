use std::cmp::min;
use std::io::{self, SeekFrom};

use vfat::{VFatFileSystem, Shared};
use traits::BlockDevice;
use vfat::fat::SharedFat;
use vfat::lock_manager::LockMode;
use vfat::lock_manager::FSObjectGuard;

pub struct ClusterChain {
    pub(crate) vfat: Shared<VFatFileSystem>,
    fat: SharedFat,
    pub(crate) first_cluster: u32,
    cluster_size_bytes: u32,
    previous_cluster: Option<u32>,
    current_cluster: Option<u32>,
    pub(crate) position: u64,
    pub(crate) guard: FSObjectGuard,
}

impl ClusterChain {
    pub fn open(vfat: Shared<VFatFileSystem>, first_cluster: u32, mode: LockMode) -> Option<ClusterChain> {
        let vfat2 = vfat.borrow();
        if let Some(guard) = vfat2.lock_manager().try_lock(first_cluster, mode) {
            Some(ClusterChain {
                fat: vfat2.fat(),
                vfat: vfat.clone(),
                first_cluster,
                cluster_size_bytes: vfat2.cluster_size_bytes(),
                current_cluster: Some(first_cluster),
                previous_cluster: None,
                position: 0,
                guard,
            })
        } else {
            None
        }
    }

    pub fn at_end(&self) -> bool {
        self.current_cluster.is_none()
    }

    fn rewind(&mut self) {
        self.position = 0;
        self.previous_cluster = None;
        self.current_cluster = Some(self.first_cluster);
    }

    fn cluster_index(&self, pos: u64) -> u64 {
        pos / self.cluster_size_bytes as u64
    }

    fn advance(&mut self, bytes: u64) -> io::Result<()> {
        let final_pos = self.position + bytes;
        while self.position < final_pos {
            if self.current_cluster.is_none() {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            let next_cluster_index = self.cluster_index(self.position) + 1;
            let next_cluster_start_pos = next_cluster_index * self.cluster_size_bytes as u64;

            if final_pos < next_cluster_start_pos {
                self.position = final_pos;
                break;
            }
            let next_cluster = self.fat.get_next_in_chain(self.current_cluster.unwrap())?;
            self.position = next_cluster_start_pos;
            self.previous_cluster = self.current_cluster;
            self.current_cluster = next_cluster;
        }
        Ok(())
    }

    fn advance_to_end(&mut self) -> io::Result<()> {
        let next_cluster_index = self.cluster_index(self.position) + 1;
        let next_cluster_start_pos = next_cluster_index * self.cluster_size_bytes as u64;
        let position = self.position;
        let cluster_size_bytes = self.cluster_size_bytes;
        self.advance(next_cluster_start_pos - position)?;
        while !self.at_end() {
            self.advance(cluster_size_bytes as u64)?;
        }
        Ok(())
    }

}

impl io::Read for ClusterChain {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut total_read_size = 0;
        loop {
            if self.current_cluster.is_none() {
                break;
            }
            let buf_tail = &mut buf[total_read_size..];

            let cluster_offset = self.position % self.cluster_size_bytes as u64;
            let read_size = min(self.cluster_size_bytes as u64 - cluster_offset, buf_tail.len() as u64);
            if read_size == 0 {
                break;
            }
            self.vfat.borrow_mut().read_cluster(self.current_cluster.unwrap(), cluster_offset as u32,
                                                &mut buf_tail[..read_size as usize])?;
            self.advance(read_size)?;
            total_read_size += read_size as usize;
        }
        Ok(total_read_size)
    }
}

impl io::Write for ClusterChain {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.guard.mode() != Some(LockMode::Write) {
            return Err(io::Error::new(io::ErrorKind::Other, "file is opened for reading only"));
        }
        let mut total_write_size = 0;
        loop {
            let buf_tail = &buf[total_write_size..];

            let cluster_offset = self.position % self.cluster_size_bytes as u64;
            let write_size = min(self.cluster_size_bytes as u64 - cluster_offset, buf_tail.len() as u64);
            if write_size == 0 {
                break;
            }

            if self.current_cluster.is_none() {
                let new_cluster = self.fat.alloc_for_chain(self.previous_cluster.unwrap())?;
                self.current_cluster = Some(new_cluster);
            }

            self.vfat.borrow_mut().write_cluster(self.current_cluster.unwrap(), cluster_offset as u32,
                                                &buf_tail[..write_size as usize])?;
            self.advance(write_size)?;
            total_write_size += write_size as usize;
        }
        Ok(total_write_size)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.vfat.borrow_mut().device.sync()
    }
}

impl io::Seek for ClusterChain {
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
            SeekFrom::Start(p) => p,
            SeekFrom::End(p) => {
                if p < 0 {
                    return Err(io::Error::from(io::ErrorKind::InvalidInput));
                }
                self.advance_to_end()?;
                if p as u64 > self.position {
                    return Err(io::Error::from(io::ErrorKind::InvalidInput));
                }
                self.position - p as u64
            },
            SeekFrom::Current(p) => {
                let r = self.position as i64 + p;
                if r < 0 {
                    return Err(io::Error::from(io::ErrorKind::InvalidInput));
                }
                r as u64
            },
        };
        let position = self.position;
        if new_pos < position {
            self.rewind();
            self.advance(new_pos)?;
        } else {
            self.advance(new_pos - position)?;
        }
        Ok(self.position)
    }
}
