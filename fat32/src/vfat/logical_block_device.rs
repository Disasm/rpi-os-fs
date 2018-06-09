use traits::BlockDevice;
use std::io;
use std::cmp::min;

pub struct LogicalBlockDevice<T: BlockDevice> {
    source: T,
    logical_sector_size: u64,
    ratio: u64,
}

impl<T: BlockDevice> LogicalBlockDevice<T> {
    fn new(source: T, logical_sector_size: u64) -> Self {
        assert!(logical_sector_size >= source.sector_size());
        assert_eq!(logical_sector_size % source.sector_size(), 0);
        let ratio = logical_sector_size / source.sector_size();

        LogicalBlockDevice {
            source, logical_sector_size, ratio
        }
    }
}

impl<T: BlockDevice> BlockDevice for LogicalBlockDevice<T> {
    fn sector_size(&self) -> u64 {
        self.logical_sector_size
    }

    fn read_sector(&mut self, n: u64, buf: &mut [u8]) -> Result<usize, io::Error> {
        let mut actual_size = 0;
        for i in 0..self.ratio {
            let buf_offset = i * self.source.sector_size();
            let mut buf_tail = &mut buf[buf_offset as usize..];
            if buf_tail.len() == 0 {
                break;
            }
            let size = self.source.read_sector(n * self.ratio + i, &mut buf_tail)?;
            if size != min(buf_tail.len(), self.source.sector_size() as usize) {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            actual_size += size;
        }
        Ok(actual_size)
    }

    // TODO: remove code duplication!
    fn write_sector(&mut self, n: u64, buf: &[u8]) -> Result<usize, io::Error> {
        let mut actual_size = 0;
        for i in 0..self.ratio {
            let buf_offset = i * self.source.sector_size();
            let buf_tail = &buf[buf_offset as usize..];
            if buf_tail.len() == 0 {
                break;
            }
            let size = self.source.write_sector(n * self.ratio + i, &buf_tail)?;
            if size != min(buf_tail.len(), self.source.sector_size() as usize) {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            actual_size += size;
        }
        Ok(actual_size)
    }
}
