use std::cmp::min;
use std::io::{self, SeekFrom};

use vfat::{VFat, Shared};
use vfat::Status;

pub struct ClusterChain {
    vfat: Shared<VFat>,
    start_cluster: u32,
    cluster_size_bytes: u32,
    previous_cluster: Option<u32>,
    current_cluster: Option<u32>,
    position: u32,
}

impl ClusterChain {
    pub fn open(mut vfat: Shared<VFat>, start_cluster: u32) -> ClusterChain {
        let cluster_size_bytes = vfat.borrow().cluster_size_bytes();
        ClusterChain {
            vfat,
            start_cluster,
            cluster_size_bytes,
            current_cluster: Some(start_cluster),
            previous_cluster: None,
            position: 0,
        }
    }

    pub fn at_end(&self) -> bool {
        self.current_cluster.is_none()
    }

    fn rewind(&mut self) {
        self.position = 0;
        self.previous_cluster = None;
        self.current_cluster = Some(self.start_cluster);
    }

    fn cluster_index(&self, pos: u32) -> u32 {
        pos / self.cluster_size_bytes
    }

    fn advance(&mut self, bytes: u32) -> io::Result<()> {
        let final_pos = self.position + bytes;
        while self.position < final_pos {
            if self.current_cluster.is_none() {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            let next_cluster_index = self.cluster_index(self.position) + 1;
            let next_cluster_start_pos = next_cluster_index * self.cluster_size_bytes;

            if final_pos < next_cluster_start_pos {
                self.position = final_pos;
                break;
            }
            let fat_entry = self.vfat.borrow_mut().fat_entry(self.current_cluster.unwrap())?;
            let next_cluster = match fat_entry.status() {
                Status::Data(new_cluster) => Some(new_cluster),
                Status::Eoc(_) => None,
                _ => {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
            };
            self.position = next_cluster_start_pos;
            self.previous_cluster = self.current_cluster;
            self.current_cluster = next_cluster;
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

            let cluster_offset = self.position % self.cluster_size_bytes;
            let read_size = min(self.cluster_size_bytes - cluster_offset, buf_tail.len() as u32);
            if read_size == 0 {
                break;
            }
            self.vfat.borrow_mut().read_cluster(self.current_cluster.unwrap(), cluster_offset, &mut buf_tail[..read_size as usize])?;
            self.advance(read_size)?;
            total_read_size += read_size as usize;
        }
        Ok(total_read_size)
    }
}

impl io::Write for ClusterChain {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        unimplemented!()
    }

    fn flush(&mut self) -> io::Result<()> {
        unimplemented!()
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
        unimplemented!("ClusterChain::seek()")
    }
}
