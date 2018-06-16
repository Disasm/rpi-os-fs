use std::io;
use std::path::Path;
use std::mem::size_of;

use vfat::{Shared, File, Dir, FatEntry, Error};
use vfat::{self, BiosParameterBlock};
use traits::{FileSystem, BlockDevice, FileSystemObject};
use vfat::logical_block_device::LogicalBlockDevice;
use std::mem;
use std::path::Component;
use vfat::Entry;

pub struct VFat {
    pub(crate) device: Box<BlockDevice>,
    pub(crate) bytes_per_sector: u16,
    pub(crate) sectors_per_cluster: u8,
    pub(crate) sectors_per_fat: u32,
    pub(crate) fat_start_sector: u64,
    pub(crate) data_start_sector: u64,
    pub(crate) root_dir_cluster: u32,
}

impl VFat {
    pub fn from<T: BlockDevice + 'static>(mut device: T) -> Result<Shared<VFat>, Error>
    {
        let ebpb = BiosParameterBlock::read_from(&mut device)?;
        let logical_block_device = LogicalBlockDevice::new(device, ebpb.bytes_per_logical_sector as u64);
        let vfat = VFat {
            device: Box::new(logical_block_device),
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

    pub(crate) fn cluster_size_bytes(&self) -> u32 {
        self.sectors_per_cluster as u32 * self.bytes_per_sector as u32
    }

    //
    //  * A method to read from an offset of a cluster into a buffer.
    //
    pub(crate) fn read_cluster(
        &mut self,
        cluster: u32,
        offset: u32,
        buf: &mut [u8]
    ) -> io::Result<()> {
        if cluster < 2 {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        if (offset + buf.len() as u32) > self.cluster_size_bytes() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }

        let cluster_sector = self.data_start_sector + (cluster as u64 - 2) * self.sectors_per_cluster as u64;
        let full_offset = cluster_sector * self.bytes_per_sector as u64 + offset as u64;
        self.device.read_by_offset(full_offset, buf)
    }



    //
    //  * A method to read all of the clusters chained from a starting cluster
    //    into a vector.
    //
//    fn read_chain(
//        &mut self,
//        start_cluster: u32,
//        buf: &mut Vec<u8>
//    ) -> io::Result<usize> {
//        let mut cluster_buffer = Vec::new();
//        cluster_buffer.resize(self.sectors_per_cluster as usize * self.bytes_per_sector as usize, 0);
//
//        let mut current_cluster = start_cluster;
//        loop {
//            self.read_cluster(current_cluster, 0, &mut cluster_buffer)?;
//            buf.extend_from_slice(&cluster_buffer);
//        }
//    }
    //
    //  * A method to return a reference to a `FatEntry` for a cluster where the
    //    reference points directly into a cached sector.
    //
    pub(crate) fn fat_entry(&mut self, cluster: u32) -> io::Result<FatEntry> {
        let max_clusters = self.sectors_per_fat * self.bytes_per_sector as u32 / size_of::<u32>() as u32;
        if cluster >= max_clusters {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut offset = (cluster as u64) * size_of::<u32>() as u64;
        offset += self.fat_start_sector * self.bytes_per_sector as u64;
        let mut buf = [0; 4];
        self.device.read_by_offset(offset, &mut buf)?;
        let entry: u32 = unsafe { mem::transmute(buf) };
        Ok(FatEntry(entry))
    }
}


impl Shared<VFat> {
    pub fn open_entry(&self, entry: &Entry) -> vfat::FileSystemObject {
        vfat::FileSystemObject::from_entry(self.clone(), entry)
    }

    pub fn root(&self) -> vfat::Dir {
        vfat::FileSystemObject::root(self.clone()).into_dir().unwrap()
    }
}

impl<'a> FileSystem for &'a Shared<VFat> {
    type File = File;
    type Dir = Dir;
    type FileSystemObject = vfat::FileSystemObject;

    fn open<P: AsRef<Path>>(self, path: P) -> io::Result<Self::FileSystemObject> {
        let mut parent = vfat::FileSystemObject::root(self.clone());
        for component in path.as_ref().components() {
            if component != Component::RootDir {
                if !parent.is_dir() {
                    return Err(io::Error::from(io::ErrorKind::NotFound));
                }
                let entry = parent.into_dir().unwrap().find(component)?;
                parent = self.open_entry(&entry);
            }
        }
        Ok(parent)
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
