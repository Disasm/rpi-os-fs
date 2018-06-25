use std::io;
use std::path::Path;

use vfat::{Shared, VFatFile, VFatDir, Error};
use vfat::BiosParameterBlock;
use traits::{FileSystem, BlockDevice, Entry, Dir};
use vfat::logical_block_device::LogicalBlockDevice;
use std::path::Component;
use vfat::VFatEntry;
use vfat::logical_block_device::SharedLogicalBlockDevice;
use std::sync::{Arc, Mutex};
use vfat::fat::SharedFat;
use vfat::lock_manager::SharedLockManager;
use std::sync::Weak;
use std::collections::HashMap;
use vfat::dir::SharedVFatDir;

pub struct VFatFileSystem {
    pub(crate) device: SharedLogicalBlockDevice,
    pub(crate) bytes_per_sector: u16,
    pub(crate) sectors_per_cluster: u8,
    pub(crate) data_start_sector: u64,
    pub(crate) root_dir_cluster: u32,
    fat: SharedFat,
    lock_manager: SharedLockManager,
    dirs: HashMap<u32, Weak<Mutex<VFatDir>>>,
}

impl VFatFileSystem {
    pub fn from(mut device: Box<BlockDevice>) -> Result<Shared<VFatFileSystem>, Error>
    {
        let ebpb = BiosParameterBlock::read_from(&mut device)?;
        let logical_block_device = LogicalBlockDevice::new(device, ebpb.bytes_per_logical_sector as u64);
        let device = Mutex::new(logical_block_device).into();
        let vfat = VFatFileSystem {
            fat: SharedFat::new(&device, &ebpb),
            device,
            bytes_per_sector: ebpb.bytes_per_logical_sector,
            sectors_per_cluster: ebpb.logical_sectors_per_cluster,
            data_start_sector: (ebpb.reserved_logical_sectors as u64) +
                (ebpb.number_of_fats as u64 * ebpb.logical_sectors_per_fat as u64),
            root_dir_cluster: ebpb.root_directory_cluster,
            lock_manager: SharedLockManager::new(),
            dirs: HashMap::new(),
        };
        Ok(Shared::new(vfat))
    }

    pub(crate) fn cluster_size_bytes(&self) -> u32 {
        self.sectors_per_cluster as u32 * self.bytes_per_sector as u32
    }

    fn get_full_offset(&self, cluster: u32, offset: u32, buf_len: usize) -> io::Result<u64> {
        if cluster < 2 {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        if (offset + buf_len as u32) > self.cluster_size_bytes() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }

        let cluster_sector = self.data_start_sector + (cluster as u64 - 2) * self.sectors_per_cluster as u64;
        Ok(cluster_sector * self.bytes_per_sector as u64 + offset as u64)
    }

    //
    //  * A method to read from an offset of a cluster into a buffer.
    //
    pub(crate) fn read_cluster(&mut self, cluster: u32, offset: u32, buf: &mut [u8]) -> io::Result<()> {
        let full_offset = self.get_full_offset(cluster, offset, buf.len())?;
        self.device.read_by_offset(full_offset, buf)
    }

    pub(crate) fn write_cluster(&mut self, cluster: u32, offset: u32, buf: &[u8]) -> io::Result<()> {
        let full_offset = self.get_full_offset(cluster, offset, buf.len())?;
        self.device.write_by_offset(full_offset, buf)
    }

    pub(crate) fn fat(&self) -> SharedFat {
        self.fat.clone()
    }

    pub(crate) fn lock_manager(&self) -> SharedLockManager {
        self.lock_manager.clone()
    }

    pub(crate) fn dir(&mut self, cluster: u32) -> Option<Arc<Mutex<VFatDir>>> {
        self.dirs.get(&cluster).and_then(|w| w.upgrade())
    }

    pub(crate) fn put_dir(&mut self, cluster: u32, dir: Arc<Mutex<VFatDir>>) {
        self.dirs.insert(cluster, Arc::downgrade(&dir));
    }

}


impl Shared<VFatFileSystem> {


    pub fn into_block_device(self) -> Box<BlockDevice> {
        let vfat = self.unwrap();
        // TODO: unwrap fat, lock manager
        vfat.fat.try_unwrap().ok().unwrap();
        Arc::try_unwrap(vfat.device).ok().unwrap().into_inner().unwrap().source
    }
}

impl FileSystem for Shared<VFatFileSystem> {
    type File = VFatFile;
    type Dir = SharedVFatDir;
    type Entry = VFatEntry;

    fn get_entry<P: AsRef<Path>>(&self, path: P) -> io::Result<Self::Entry> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(io::Error::new(io::ErrorKind::Other, "relative paths are not supported"));
        }
        let mut parent = VFatDir::root(self.clone());
        let mut iterator = path.components().peekable();
        while let Some(component) = iterator.next() {
            if component == Component::RootDir {
                continue;
            }
            let entry = parent.find(component)?;
            if iterator.peek().is_none() { // last iteration
                return Ok(entry);
            } else { // not last iteration
                parent = entry.open_dir()?;
            }
        }
        unreachable!()
    }

    fn root(&self) -> io::Result<SharedVFatDir> {
        Ok(VFatDir::root(self.clone()))
    }

    fn create_file<P: AsRef<Path>>(&self, _path: P) -> io::Result<Self::File> {
        unimplemented!()
    }

    fn create_dir<P>(&self, _path: P, _parents: bool) -> io::Result<Self::Dir>
        where P: AsRef<Path>
    {
        unimplemented!()
    }

    fn rename<P, Q>(&self, _from: P, _to: Q) -> io::Result<()>
        where P: AsRef<Path>, Q: AsRef<Path>
    {
        unimplemented!()
    }

    fn remove<P: AsRef<Path>>(&self, _path: P, _children: bool) -> io::Result<()> {
        unimplemented!()
    }
}
