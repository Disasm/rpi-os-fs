use std::fmt;
use vfat::Shared;
use vfat::VFatFileSystem;
use std::io;
use traits::BlockDevice;

#[derive(Debug, PartialEq, Clone)]
pub enum Status {
    /// The FAT entry corresponds to an unused (free) cluster.
    Free,
    /// The FAT entry/cluster is reserved.
    Reserved,
    /// The FAT entry corresponds to a valid data cluster. The next cluster in
    /// the chain is `Cluster`.
    Data(u32),
    /// The FAT entry corresponds to a bad (disk failed) cluster.
    Bad,
    /// The FAT entry corresponds to a valid data cluster. The corresponding
    /// cluster is the last in its chain.
    Eoc(u32)
}

#[repr(C, packed)]
#[derive(Clone)]
pub struct FatEntry(pub u32);

impl FatEntry {
    /// Returns the `Status` of the FAT entry `self`.
    pub fn status(&self) -> Status {
        let cluster = self.0 & !(0xF << 28);
        match cluster {
            0x0000000 => Status::Free,
            0x0000001 => Status::Reserved,
            2..=0xFFFFFEF => Status::Data(cluster),
            0xFFFFFF0..=0xFFFFFF6 => Status::Reserved,
            0xFFFFFF7 => Status::Bad,
            0xFFFFFF8..=0xFFFFFFF => Status::Eoc(cluster),
            _ => unreachable!(),
        }
    }
}

impl fmt::Debug for FatEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FatEntry")
            .field("value", &self.0)
            .field("status", &self.status())
            .finish()
    }
}


struct SingleFat {
    vfat: Shared<VFatFileSystem>,
    offset: u64,
    size: u32,
}

impl SingleFat {
    const FAT_ENTRY_SIZE: u64 = 4;

    fn new(vfat: Shared<VFatFileSystem>, index: u8) -> SingleFat {
        let (offset, size) = {
            let vfat = vfat.borrow();

            let fat_size_bytes = vfat.sectors_per_fat as u64 * vfat.bytes_per_sector as u64;
            let size = (fat_size_bytes / Self::FAT_ENTRY_SIZE) as u32;
            let first_fat_offset = vfat.fat_start_sector as u64 * vfat.bytes_per_sector as u64;
            let offset = first_fat_offset + index as u64 * fat_size_bytes;
            (offset, size)
        };
        Self {
            offset, size, vfat,
        }
    }

    fn get(&self, cluster: u32) -> io::Result<FatEntry> {
        if cluster >= self.size {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut buf = [0; 4];
        self.vfat.borrow().device.read_by_offset(self.offset + cluster as u64 * Self::FAT_ENTRY_SIZE, &mut buf)?;
        let entry: u32 = unsafe { ::std::mem::transmute(buf) };
        Ok(FatEntry(entry))
    }

    fn set(&mut self, cluster: u32, entry: u32) -> io::Result<()> {
        if cluster >= self.size {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let buf: [u8; 4] = unsafe { ::std::mem::transmute(entry) };
        self.vfat.borrow_mut().device.write_by_offset(self.offset + cluster as u64 * Self::FAT_ENTRY_SIZE, &buf)
    }

    fn size(&self) -> u32 {
        self.size
    }
}

pub struct Fat {
    fats: Vec<SingleFat>,
}

impl Fat {
    pub(crate) fn new(vfat: Shared<VFatFileSystem>) -> Self {
        let number_of_fats = vfat.borrow().number_of_fats;
        Fat {
            fats: (0..number_of_fats).map(|i| SingleFat::new(vfat.clone(), i)).collect(),
        }
    }

    fn get(&self, cluster: u32) -> io::Result<FatEntry> {
        self.fats[0].get(cluster)
    }

    fn set(&mut self, cluster: u32, entry: u32) -> io::Result<()> {
        for fat in &mut self.fats {
            fat.set(cluster, entry)?;
        }
        Ok(())
    }

    fn size(&self) -> u32 {
        self.fats[0].size()
    }

    fn alloc(&mut self, value: u32) -> io::Result<u32> {
        for i in 2..self.size() {
            if self.get(i)?.status() == Status::Free {
                self.set(i, value)?;
                return Ok(i);
            }
        }
        Err(io::Error::new(io::ErrorKind::Other, "no free clusters"))
    }

    pub fn new_chain(&mut self) -> io::Result<u32> {
        self.alloc(0xFFFFFFF)
    }

    pub fn alloc_for_chain(&mut self, last_cluster: u32) -> io::Result<u32> {
        let new_last_cluster = self.alloc(0xFFFFFFF)?;
        self.set(last_cluster, new_last_cluster)?;
        Ok(new_last_cluster)
    }

    pub fn get_next_in_chain(&self, cluster: u32) -> io::Result<Option<u32>> {
        match self.get(cluster)?.status() {
            Status::Data(next) => Ok(Some(next)),
            Status::Eoc(_) => Ok(None),
            _ => Err(io::Error::from(io::ErrorKind::InvalidData))
        }
    }

    pub fn free_chain(&mut self, first_cluster: u32) -> io::Result<()> {
        let mut current_cluster = first_cluster;
        loop {
            match self.get(current_cluster)?.status() {
                Status::Data(next) => {
                    self.set(current_cluster, 0)?;
                    current_cluster = next;
                },
                Status::Eoc(_) => {
                    self.set(current_cluster, 0)?;
                    return Ok(());
                }
                _ => return Err(io::Error::from(io::ErrorKind::InvalidData)),
            }
        }
    }

    pub fn truncate_chain(&mut self, last_cluster: u32) -> io::Result<()> {
        match self.get(last_cluster)?.status() {
            Status::Data(next) => {
                self.free_chain(next)?;
                self.set(last_cluster, 0xFFFFFFF)?;
            }
            Status::Eoc(_) => {}
            _ => return Err(io::Error::from(io::ErrorKind::InvalidData))
        }
        Ok(())
    }
}
