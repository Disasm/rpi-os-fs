use std::ops::Range;
use traits::BlockDevice;
use std::io;

pub type SectorRange = Range<u64>;


pub struct Partition<T: BlockDevice> {
    source: T,
    sector_range: SectorRange,
}

impl<T: BlockDevice> Partition<T> {
    pub fn new(source: T, sector_range: SectorRange) -> Self {
        Partition {
            source, sector_range
        }
    }

    fn to_source_sector(&self, n: u64) -> Result<u64, io::Error> {
        let source_sector = n + self.sector_range.start;
        if !self.sector_range.contains(source_sector) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        Ok(source_sector)
    }
}

impl<T: BlockDevice> BlockDevice for Partition<T> {
    fn sector_size(&self) -> u64 {
        self.source.sector_size()
    }

    fn read_sector(&self, n: u64, buf: &mut [u8]) -> Result<usize, io::Error> {
        let m = self.to_source_sector(n)?;
        self.source.read_sector(m, buf)
    }

    fn write_sector(&mut self, n: u64, buf: &[u8]) -> Result<usize, io::Error> {
        let m = self.to_source_sector(n)?;
        self.source.write_sector(m, buf)
    }

    fn sync(&mut self) -> io::Result<()> {
        self.source.sync()
    }
}
