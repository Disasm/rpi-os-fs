use std::fmt;
use vfat::*;

use self::Status::*;

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
