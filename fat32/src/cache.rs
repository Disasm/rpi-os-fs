use traits::BlockDevice;
use std::io;
use std::collections::HashMap;
use std::cell::RefCell;


#[derive(Debug)]
struct CacheEntry {
    data: Vec<u8>,
    is_dirty: bool
}

struct Cache(HashMap<u64, CacheEntry>);

impl Cache {
    fn cache_entry<T: BlockDevice>(&mut self, sector: u64, device: &T) -> io::Result<&mut CacheEntry> {
        if !self.0.contains_key(&sector) {
            let mut cache_entry = CacheEntry {
                data: Vec::new(),
                is_dirty: false,
            };
            cache_entry.data.resize(device.sector_size() as usize, 0);
            device.read_sector(sector, &mut cache_entry.data)?;
            self.0.insert(sector, cache_entry);
        }
        Ok(self.0.get_mut(&sector).unwrap())
    }
}

pub struct CachedDevice<T: BlockDevice> {
    source: T,
    cache: RefCell<Cache>,
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
            cache: RefCell::new(Cache(HashMap::new())),
        }
    }
}

impl<T: BlockDevice> BlockDevice for CachedDevice<T> {
    fn sector_size(&self) -> u64 {
        self.source.sector_size()
    }

    fn read_sector(&self, n: u64, buf: &mut [u8]) -> Result<(), io::Error> {
        if buf.len() as u64 != self.sector_size() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut cache = self.cache.borrow_mut();
        let cache_entry = cache.cache_entry(n, &self.source)?;
        buf.copy_from_slice(&cache_entry.data);
        Ok(())

    }

    fn write_sector(&mut self, n: u64, buf: &[u8]) -> Result<(), io::Error> {
        if buf.len() as u64 != self.sector_size() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut cache = self.cache.borrow_mut();
        let cache_entry = cache.cache_entry(n, &self.source)?;
        cache_entry.data.copy_from_slice(&buf);
        cache_entry.is_dirty = true;
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        for (sector, entry) in &mut self.cache.borrow_mut().0 {
            if entry.is_dirty {
                self.source.write_sector(*sector, &entry.data)?;
                entry.is_dirty = false;
            }
        }
        Ok(())
    }
}
