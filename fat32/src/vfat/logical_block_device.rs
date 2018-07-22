use traits::BlockDevice;
use std::io;
use std::cmp::min;
use arc_mutex::ArcMutex;

pub struct LogicalBlockDevice {
    pub(crate) source: Box<BlockDevice>,
    logical_sector_size: u64,
}

impl LogicalBlockDevice {
    pub fn new(source: Box<BlockDevice>, logical_sector_size: u64) -> Self {
        assert!(logical_sector_size >= source.sector_size());
        assert_eq!(logical_sector_size % source.sector_size(), 0);

        LogicalBlockDevice {
            source, logical_sector_size
        }
    }
}

impl BlockDevice for LogicalBlockDevice {
    fn sector_size(&self) -> u64 {
        self.logical_sector_size
    }

    fn read_sector(&self, sector: u64, buf: &mut [u8]) -> Result<(), io::Error> {
        let size = min(buf.len(), self.sector_size() as usize);
        let buf2 = &mut buf[..size];
        let source_offset = sector * self.sector_size();
        self.source.read_by_offset(source_offset, buf2)?;
        Ok(())
    }

    fn write_sector(&mut self, sector: u64, buf: &[u8]) -> Result<(), io::Error> {
        let size = min(buf.len(), self.sector_size() as usize);
        let buf2 = &buf[..size];
        let source_offset = sector * self.sector_size();
        self.source.write_by_offset(source_offset, buf2)?;
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.source.sync()
    }
}

pub type SharedLogicalBlockDevice = ArcMutex<LogicalBlockDevice>;
