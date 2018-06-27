use traits::{Entry, Metadata};
use vfat::metadata::VFatMetadata;
use vfat::VFatDir;
use std::io;
use vfat::lock_manager::FSObjectGuard;
use vfat::VFatFile;
use vfat::lock_manager::LockMode;
use vfat::Shared;
use vfat::VFatFileSystem;
use traits::FileOpenMode;
use vfat::dir::SharedVFatDir;

pub struct VFatEntry {
    pub(crate) name: String,
    pub(crate) metadata: VFatMetadata,
    pub(crate) dir: SharedVFatDir,
    pub(crate) first_entry_index: u64,
    pub(crate) regular_entry_index: u64,

    #[allow(unused)]
    pub(crate) ref_guard: FSObjectGuard,
}

impl VFatEntry {
    pub(crate) fn vfat(&self) -> Shared<VFatFileSystem> {
        self.dir.lock().unwrap().vfat.clone()
    }

    pub(crate) fn set_file_size(&mut self, size: u32) -> io::Result<()> {
        assert!(!self.metadata.is_dir());
        self.dir.lock().unwrap().set_file_size(self.regular_entry_index, size)
    }

    pub(crate) fn current_file_size(&self) -> io::Result<u32> {
        self.dir.lock().unwrap().get_file_size(self.regular_entry_index)
    }
}

impl Clone for VFatEntry {
    fn clone(&self) -> Self {
        let vfat = self.vfat();
        let ref_guard = vfat.borrow().lock_manager().lock(self.metadata.first_cluster, LockMode::Ref);
        Self {
            name: self.name.clone(),
            metadata: self.metadata.clone(),
            dir: self.dir.clone(),
            first_entry_index: self.first_entry_index,
            regular_entry_index: self.regular_entry_index,
            ref_guard,
        }
    }
}

impl Entry for VFatEntry {
    type Metadata = VFatMetadata;
    type File = VFatFile;
    type Dir = SharedVFatDir;

    fn name(&self) -> &str {
        &self.name
    }

    fn parent(&self) -> SharedVFatDir {
        self.dir.clone()
    }

    fn metadata(&self) -> &VFatMetadata {
        &self.metadata
    }

    fn is_file(&self) -> bool {
        !self.metadata.is_dir()
    }

    fn is_dir(&self) -> bool {
        self.metadata.is_dir()
    }

    fn open_file(&self, mode: FileOpenMode) -> io::Result<VFatFile> {
        if !self.metadata.is_dir() {
            VFatFile::from_entry(self, mode)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "not a regular file"))
        }
    }

    fn open_dir(&self) -> io::Result<SharedVFatDir> {
        if self.metadata.is_dir() {
            self.vfat().get_dir(self.metadata.first_cluster, Some(self.clone())).ok_or_else(|| io::Error::from(io::ErrorKind::PermissionDenied))
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "not a directory"))
        }
    }
}
