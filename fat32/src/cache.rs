use traits::BlockDevice;
use std::io;
use std::collections::HashMap;
use std::cmp::min;


#[derive(Debug)]
struct CacheEntry {
    data: Vec<u8>,
    is_dirty: bool
}


pub struct CachedDevice<T: BlockDevice> {
    source: T,
    cache: HashMap<u64, CacheEntry>,
}

impl<T: BlockDevice> Drop for CachedDevice<T> {
    fn drop(&mut self) {
        self.sync().unwrap();
    }
}

impl<T: BlockDevice> CachedDevice<T> {
    pub fn new(source: T) -> Self {
        CachedDevice {
            source,
            cache: HashMap::new(),
        }
    }

    pub fn sync(&mut self) -> io::Result<()> {
        for (sector, entry) in &mut self.cache {
            if entry.is_dirty {
                self.source.write_sector(*sector, &entry.data)?;
                entry.is_dirty = false;
            }
        }
        Ok(())
    }

    fn cache_entry(&mut self, sector: u64) -> io::Result<&mut CacheEntry> {
        if !self.cache.contains_key(&sector) {
            let mut cache_entry = CacheEntry {
                data: Vec::new(),
                is_dirty: false,
            };
            cache_entry.data.resize(self.source.sector_size() as usize, 0);
            self.source.read_sector(sector, &mut cache_entry.data)?;
            self.cache.insert(sector, cache_entry);
        }
        Ok(self.cache.get_mut(&sector).unwrap())
    }


}

impl<T: BlockDevice> BlockDevice for CachedDevice<T> {
    fn sector_size(&self) -> u64 {
        self.source.sector_size()
    }

    fn read_sector(&mut self, n: u64, buf: &mut [u8]) -> Result<usize, io::Error> {
        let cache_entry = self.cache_entry(n)?;
        let bytes = min(cache_entry.data.len(), buf.len());
        buf[..bytes].copy_from_slice(&cache_entry.data[..bytes]);
        return Ok(bytes);

    }

    fn write_sector(&mut self, n: u64, buf: &[u8]) -> Result<usize, io::Error> {
        let cache_entry = self.cache_entry(n)?;
        let bytes = min(cache_entry.data.len(), buf.len());
        cache_entry.data[..bytes].copy_from_slice(&buf[..bytes]);
        cache_entry.is_dirty = true;
        return Ok(bytes);
    }
}
