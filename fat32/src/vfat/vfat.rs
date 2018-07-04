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
use vfat::lock_manager::LockMode;
use fallible_iterator::FallibleIterator;
use vfat::metadata::VFatMetadata;
use vfat::metadata::Attributes;
use traits::FileOpenMode;
use vfat::lock_manager::FSObjectGuard;

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

}


impl Shared<VFatFileSystem> {
    fn lock_entry_for_deletion(&self, entry: &mut VFatEntry) -> io::Result<FSObjectGuard> {
        if entry.is_file() {
            entry.ref_guard.take();
            let mut lock = self.borrow().lock_manager().try_lock(entry.metadata.first_cluster, LockMode::Delete)
                .ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, "can't get delete lock for file"))?;
            Ok(lock.take())
        } else {
            let dir = VFatDir::open(self.clone(), entry.metadata.first_cluster, Some(entry.clone()))
                .ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, "failed to lock dir before deleting it"))?;
            if dir.entries()?.next()?.is_some() {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "can't remove non-empty dir"));
            }
            let mut dir = dir.0.lock().unwrap();
            Ok(dir.chain.guard.take())
        }
    }

    pub fn into_block_device(self) -> Box<BlockDevice> {
        let vfat = self.unwrap();
        // TODO: unwrap fat, lock manager
        vfat.fat.try_unwrap().ok().unwrap();
        Arc::try_unwrap(vfat.device).ok().unwrap().into_inner().unwrap().source
    }

    pub(crate) fn get_dir(&self, first_cluster: u32, entry: Option<VFatEntry>) -> Option<SharedVFatDir> {
        if let Some(r) = self.borrow_mut().dirs.get(&first_cluster).and_then(|w| w.upgrade()) {
            return Some(SharedVFatDir(r));
        }
        if let Some(dir) = VFatDir::open(self.clone(), first_cluster, entry) {
            self.borrow_mut().dirs.insert(first_cluster, Arc::downgrade(&dir.0));
            Some(dir)
        } else {
            None
        }
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
        let mut parent = self.root().unwrap();
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
        let first_cluster = self.borrow().root_dir_cluster;
        Self::get_dir(self, first_cluster, None).ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, "can't get root dir"))
    }

    fn create_file<P: AsRef<Path>>(&self, path: P) -> io::Result<Self::File> {
        let path = path.as_ref();
        if let Some(parent_dir) = path.parent() {
            let dir = self.open_dir(parent_dir)?;
            let file_name = path.file_name().unwrap().to_str().ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
            let current_time = ::chrono::offset::Local::now().naive_local();
            let first_cluster = self.borrow_mut().fat.new_chain()?;
            let metadata = VFatMetadata {
                attributes: Attributes::new(false),
                created: current_time,
                accessed: current_time.date(),
                modified: current_time,
                first_cluster,
                size: 0,
            };
            let entry = dir.create_entry(file_name, &metadata)?;
            entry.open_file(FileOpenMode::Write)
        } else {
            Err(io::Error::new(io::ErrorKind::AlreadyExists, "invalid file path"))
        }
    }

    fn create_dir<P>(&self, path: P) -> io::Result<Self::Dir>
        where P: AsRef<Path>
    {
        let path = path.as_ref();
        if let Some(parent_dir) = path.parent() {
            let dir = self.open_dir(parent_dir)?;
            let file_name = path.file_name().unwrap().to_str().ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
            let current_time = ::chrono::offset::Local::now().naive_local();
            let first_cluster = self.borrow_mut().fat.new_chain()?;
            let metadata = VFatMetadata {
                attributes: Attributes::new(true),
                created: current_time,
                accessed: current_time.date(),
                modified: current_time,
                first_cluster,
                size: 0,
            };
            let entry = dir.create_entry(file_name, &metadata)?;
            let dir = entry.open_dir()?;
            dir.0.lock().unwrap().init_empty(current_time)?;
            Ok(dir)
        } else {
            Err(io::Error::new(io::ErrorKind::AlreadyExists, "invalid directory path"))
        }
    }

    fn rename<P, Q>(&self, from: P, to: Q) -> io::Result<()>
        where P: AsRef<Path>, Q: AsRef<Path>
    {
        let from = from.as_ref();
        let to = to.as_ref();

        let mut entry = self.get_entry(from)?;
        let _lock = self.lock_entry_for_deletion(&mut entry)?;

        let new_parent_path = if let Some(p) = to.parent() {
            p
        } else {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "invalid path"));
        };

        let new_parent = self.open_dir(new_parent_path)?;
        let file_name = to.file_name().unwrap().to_str().ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        new_parent.0.lock().unwrap().create_entry(file_name, &entry.metadata)?;
        entry.dir.0.lock().unwrap().remove_entry(&entry)?;
        Ok(())
    }

    fn remove_entry(&self, mut entry: VFatEntry) -> io::Result<()> {
        let _lock = self.lock_entry_for_deletion(&mut entry)?;
        entry.dir.0.lock().unwrap().remove_entry(&entry)?;
        self.borrow_mut().fat.free_chain(entry.metadata.first_cluster)
    }
}

