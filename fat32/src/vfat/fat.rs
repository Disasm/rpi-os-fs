use std::fmt;
use vfat::Shared;
use vfat::VFat;
use std::io;

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
    vfat: Shared<VFat>,
    bytes_per_sector: u16,
    sectors_per_fat: u32,
    fat_start_sector: u64,
}

impl SingleFat {
    fn get(&self, cluster: u32) -> io::Result<FatEntry> {
        unimplemented!()
    }

    fn set(&mut self, cluster: u32, entry: u32) -> io::Result<()> {
        unimplemented!()
    }

    fn size(&self) -> u32 {
        unimplemented!()
    }
}

pub struct Fat {
    fats: Vec<SingleFat>,
}

impl Fat {
    pub(crate) fn new(vfat: Shared<VFat>) {
        unimplemented!()
    }

    pub fn get(&self, cluster: u32) -> io::Result<FatEntry> {
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

    pub fn append_to_chain(&mut self, last_cluster: u32) -> io::Result<u32> {
        let new_last_cluster = self.alloc(0xFFFFFFF)?;
        self.set(last_cluster, new_last_cluster)?;
        Ok(new_last_cluster)
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
