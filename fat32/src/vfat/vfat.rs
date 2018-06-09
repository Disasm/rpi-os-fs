use std::io;
use std::path::Path;
use std::mem::size_of;
use std::cmp::min;

use util::SliceExt;
use vfat::{Shared, File, Dir, Entry, FatEntry, Error, Status};
use vfat::{BiosParameterBlock};
use traits::{FileSystem, BlockDevice};
use vfat::logical_block_device::LogicalBlockDevice;
use std::mem;

pub struct VFat<T: BlockDevice> {
    device: LogicalBlockDevice<T>,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    sectors_per_fat: u32,
    fat_start_sector: u64,
    data_start_sector: u64,
    root_dir_cluster: u32,
}

impl<T: BlockDevice> VFat<T> {
    pub fn from(mut device: T) -> Result<Shared<VFat<T>>, Error>
    {
        let ebpb = BiosParameterBlock::read_from(&mut device)?;
        let logical_block_device = LogicalBlockDevice::new(device, ebpb.bytes_per_logical_sector as u64);
        let vfat = VFat {
            device: logical_block_device,
            bytes_per_sector: ebpb.bytes_per_logical_sector,
            sectors_per_cluster: ebpb.logical_sectors_per_cluster,
            sectors_per_fat: ebpb.logical_sectors_per_fat,
            fat_start_sector: ebpb.reserved_logical_sectors as u64,
            data_start_sector: (ebpb.reserved_logical_sectors as u64) +
                (ebpb.number_of_fats as u64 * ebpb.logical_sectors_per_fat as u64),
            root_dir_cluster: ebpb.root_directory_cluster,
        };
        Ok(Shared::new(vfat))
    }

    // TODO: The following methods may be useful here:
    //
    //  * A method to read from an offset of a cluster into a buffer.
    //
    fn read_cluster(
        &mut self,
        cluster: u32,
        offset: usize,
        buf: &mut [u8]
    ) -> io::Result<usize> {
        unimplemented!()
    }
    //
    //  * A method to read all of the clusters chained from a starting cluster
    //    into a vector.
    //
    //    fn read_chain(
    //        &mut self,
    //        start: Cluster,
    //        buf: &mut Vec<u8>
    //    ) -> io::Result<usize>;
    //
    //  * A method to return a reference to a `FatEntry` for a cluster where the
    //    reference points directly into a cached sector.
    //
    fn fat_entry(&mut self, cluster: u32) -> io::Result<FatEntry> {
        let mut offset = (cluster as u64) * size_of::<u32>() as u64;
        offset += self.fat_start_sector * self.bytes_per_sector as u64;
        let mut buf = [0; 4];
        self.device.read_by_offset(offset, &mut buf)?;
        let entry: u32 = unsafe { mem::transmute(buf) };
        Ok(FatEntry(entry))
    }
}

impl<'a, T: BlockDevice> FileSystem for &'a Shared<VFat<T>> {
    type File = ::traits::Dummy;
    type Dir = ::traits::Dummy;
    type Entry = ::traits::Dummy;

    fn open<P: AsRef<Path>>(self, path: P) -> io::Result<Self::Entry> {
        unimplemented!("FileSystem::open()")
    }

    fn create_file<P: AsRef<Path>>(self, _path: P) -> io::Result<Self::File> {
        unimplemented!("read only file system")
    }

    fn create_dir<P>(self, _path: P, _parents: bool) -> io::Result<Self::Dir>
        where P: AsRef<Path>
    {
        unimplemented!("read only file system")
    }

    fn rename<P, Q>(self, _from: P, _to: Q) -> io::Result<()>
        where P: AsRef<Path>, Q: AsRef<Path>
    {
        unimplemented!("read only file system")
    }

    fn remove<P: AsRef<Path>>(self, _path: P, _children: bool) -> io::Result<()> {
        unimplemented!("read only file system")
    }
}
