use std::fmt;
use std::io;
use traits::BlockDevice;
use vfat::logical_block_device::SharedLogicalBlockDevice;
use vfat::BiosParameterBlock;
use byteorder::{LittleEndian, ByteOrder};
use arc_mutex::ArcMutex;

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
    device: SharedLogicalBlockDevice,
    offset: u64,
    size: u32,
}

impl SingleFat {
    const FAT_ENTRY_SIZE: u64 = 4;

    fn new(device: SharedLogicalBlockDevice, params: &BiosParameterBlock, index: u8) -> SingleFat {
        let fat_size_bytes = params.logical_sectors_per_fat as u64 * params.bytes_per_logical_sector as u64;
        let size = (fat_size_bytes / Self::FAT_ENTRY_SIZE) as u32;
        let first_fat_offset = params.reserved_logical_sectors as u64 * params.bytes_per_logical_sector as u64;
        let offset = first_fat_offset + index as u64 * fat_size_bytes;
        Self {
            offset, size, device,
        }
    }

    fn get(&self, cluster: u32) -> io::Result<FatEntry> {
        if cluster >= self.size {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut buf = [0; 4];
        self.device.read_by_offset(self.offset + cluster as u64 * Self::FAT_ENTRY_SIZE, &mut buf)?;
        let entry = LittleEndian::read_u32(&buf);
        Ok(FatEntry(entry))
    }

    fn set(&mut self, cluster: u32, entry: u32) -> io::Result<()> {
        if cluster >= self.size {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut buf = [0; 4];
        LittleEndian::write_u32(&mut buf, entry);
        self.device.write_by_offset(self.offset + cluster as u64 * Self::FAT_ENTRY_SIZE, &buf)
    }

    fn size(&self) -> u32 {
        self.size
    }
}

pub struct Fat {
    fats: Vec<SingleFat>,
}

impl Fat {
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
}

#[derive(Clone)]
pub struct SharedFat(ArcMutex<Fat>);

impl SharedFat {
    pub fn new(device: &SharedLogicalBlockDevice, params: &BiosParameterBlock) -> Self {
        let fat = Fat {
            fats: (0..params.number_of_fats).map(|i| SingleFat::new(device.clone(), params, i)).collect(),
        };
        SharedFat(ArcMutex::new(fat))
    }

    pub(crate) fn unwrap(self) -> ArcMutex<Fat> {
        self.0
    }

    pub fn new_chain(&mut self) -> io::Result<u32> {
        let mut fat = self.0.lock();
        fat.alloc(0xFFFFFFF)
    }

    pub fn alloc_for_chain(&mut self, last_cluster: u32) -> io::Result<u32> {
        let mut fat = self.0.lock();
        let new_last_cluster = fat.alloc(0xFFFFFFF)?;
        fat.set(last_cluster, new_last_cluster)?;
        Ok(new_last_cluster)
    }

    pub fn get_next_in_chain(&self, cluster: u32) -> io::Result<Option<u32>> {
        let fat = self.0.lock();
        match fat.get(cluster)?.status() {
            Status::Data(next) => Ok(Some(next)),
            Status::Eoc(_) => Ok(None),
            _ => Err(io::Error::from(io::ErrorKind::InvalidData))
        }
    }

    pub fn free_chain(&mut self, first_cluster: u32) -> io::Result<()> {
        let mut fat = self.0.lock();
        fat.free_chain(first_cluster)
    }

    // TODO: add set_len to File
    #[allow(dead_code)]
    pub fn truncate_chain(&mut self, last_cluster: u32) -> io::Result<()> {
        let mut fat = self.0.lock();
        match fat.get(last_cluster)?.status() {
            Status::Data(next) => {
                fat.free_chain(next)?;
                fat.set(last_cluster, 0xFFFFFFF)?;
            }
            Status::Eoc(_) => {}
            _ => return Err(io::Error::from(io::ErrorKind::InvalidData))
        }
        Ok(())
    }
}
