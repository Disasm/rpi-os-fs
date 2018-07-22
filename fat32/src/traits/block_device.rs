use std::io;
use std::cmp::min;
use std::ops::Range;
use std::ops::{Deref, DerefMut};
use arc_mutex::ArcMutex;

struct IOOperationChunk {
    sector: u64,
    buf_offset: usize,
    sector_offset: usize,
    size: usize,
}
impl IOOperationChunk {
    fn buf_range(&self) -> Range<usize> {
        self.buf_offset..self.buf_offset + self.size
    }
    fn sector_range(&self) -> Range<usize> {
        self.sector_offset..self.sector_offset + self.size
    }
}

struct IOOperationIterator {
    sector_size: usize,
    buf_size: usize,
    current_sector: u64,
    current_buf_offset: usize,
    current_sector_offset: usize,

}

impl IOOperationIterator {
    fn new (sector_size: usize,
            buf_size: usize,
            offset: u64) -> IOOperationIterator {
        IOOperationIterator {
            sector_size,
            buf_size,
            current_sector: offset / sector_size as u64,
            current_sector_offset: (offset % sector_size as u64) as usize,
            current_buf_offset: 0,
        }
    }
}

impl Iterator for IOOperationIterator {
    type Item = IOOperationChunk;
    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        let size = min(self.buf_size - self.current_buf_offset,
                       self.sector_size - self.current_sector_offset);
        if size == 0 {
            return None;
        }
        let result = IOOperationChunk {
            sector: self.current_sector,
            buf_offset: self.current_buf_offset,
            sector_offset: self.current_sector_offset,
            size: size,
        };

        self.current_sector += 1;
        self.current_buf_offset += size;
        self.current_sector_offset = 0;
        Some(result)
    }
}




/// Trait implemented by devices that can be read/written in sector
/// granularities.
pub trait BlockDevice: Send {
    /// Sector size in bytes. Must be a multiple of 512 >= 512. Defaults to 512.
    fn sector_size(&self) -> u64 {
        512
    }

    /// Read sector number `n` into `buf`.
    ///
    /// `self.sector_size()` or `buf.len()` bytes, whichever is less, are read
    /// into `buf`. The number of bytes read is returned.
    ///
    /// # Errors
    ///
    /// Returns an error if seeking or reading from `self` fails.
    fn read_sector(&self, sector: u64, buf: &mut [u8]) -> io::Result<()>;

    fn read_by_offset(&self, offset_bytes: u64, buf: &mut [u8]) -> io::Result<()> {
        let mut read_sector_buf = Vec::new();
        read_sector_buf.resize(self.sector_size() as usize, 0);
        for chunk in IOOperationIterator::new(self.sector_size() as usize,
                                              buf.len(), offset_bytes) {
            self.read_sector(chunk.sector, &mut read_sector_buf)?;
            buf[chunk.buf_range()].copy_from_slice(&read_sector_buf[chunk.sector_range()]);
        }
        Ok(())
    }

    fn write_by_offset(&mut self, offset_bytes: u64, buf: &[u8]) -> io::Result<()> {
        let mut read_sector_buf = Vec::new();
        read_sector_buf.resize(self.sector_size() as usize, 0);
        for chunk in IOOperationIterator::new(self.sector_size() as usize,
                                              buf.len(), offset_bytes) {
            let buf_slice = &buf[chunk.buf_range()];
            if chunk.size == self.sector_size() as usize {
                self.write_sector(chunk.sector, buf_slice)?;
            } else {
                self.read_sector(chunk.sector, &mut read_sector_buf)?;
                read_sector_buf[chunk.sector_range()].copy_from_slice(buf_slice);
                self.write_sector(chunk.sector, &read_sector_buf)?;
            }
        }
        Ok(())
    }

//    /// Append sector number `n` into `vec`.
//    ///
//    /// `self.sector_size()` bytes are appended to `vec`. The number of bytes
//    /// read is returned.
//    ///
//    /// # Errors
//    ///
//    /// Returns an error if seeking or reading from `self` fails.
//    fn read_all_sector(&mut self, sector: u64, vec: &mut Vec<u8>) -> io::Result<usize> {
//        let sector_size = self.sector_size() as usize;
//
//        let start = vec.len();
//        let available = vec.capacity() - start;
//        if available < sector_size {
//            vec.reserve(sector_size - available);
//        }
//
//        unsafe { vec.set_len(start + sector_size); }
//        let read = self.read_sector(n, &mut vec[start..])?;
//        unsafe { vec.set_len(start + read); }
//        Ok(read)
//    }

    /// Overwrites sector `n` with the contents of `buf`.
    ///
    /// `self.sector_size()` or `buf.len()` bytes, whichever is less, are written
    /// to the sector. The number of byte written is returned.
    ///
    /// # Errors
    ///
    /// Returns an error if seeking or writing to `self` fails. Returns an
    /// error of `UnexpectedEof` if the length of `buf` is less than
    /// `self.sector_size()`.
    fn write_sector(&mut self, sector: u64, buf: &[u8]) -> io::Result<()>;

    fn sync(&mut self) -> io::Result<()>;
}

/*impl<'a, T: BlockDevice> BlockDevice for &'a mut T {
    fn read_sector(&mut self, n: u64, buf: &mut [u8]) -> io::Result<usize> {
        (*self).read_sector(n, buf)
    }

    fn write_sector(&mut self, n: u64, buf: &[u8]) -> io::Result<usize> {
        (*self).write_sector(n, buf)
    }
}*/

impl BlockDevice for Box<BlockDevice> {
    fn sector_size(&self) -> u64 {
        self.deref().sector_size()
    }

    fn read_sector(&self, sector: u64, buf: &mut [u8]) -> io::Result<()> {
        self.deref().read_sector(sector, buf)
    }

    fn write_sector(&mut self, sector: u64, buf: &[u8]) -> io::Result<()> {
        self.deref_mut().write_sector(sector, buf)
    }

    fn sync(&mut self) -> io::Result<()> {
        self.deref_mut().sync()
    }
}

impl<T: BlockDevice> BlockDevice for ArcMutex<T> {
    fn sector_size(&self) -> u64 {
        self.lock().sector_size()
    }

    fn read_sector(&self, sector: u64, buf: &mut [u8]) -> io::Result<()> {
        self.lock().read_sector(sector, buf)
    }

    fn write_sector(&mut self, sector: u64, buf: &[u8]) -> io::Result<()> {
        self.lock().write_sector(sector, buf)
    }

    fn sync(&mut self) -> io::Result<()> {
        self.lock().sync()
    }
}
