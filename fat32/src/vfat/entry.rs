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

pub struct VFatEntry {
    pub(crate) name: String,
    pub(crate) metadata: VFatMetadata,
    pub(crate) dir: VFatDir,
    pub(crate) regular_entry_index: u64,
    pub(crate) ref_guard: FSObjectGuard,
}

impl VFatEntry {
    pub(crate) fn vfat(&self) -> Shared<VFatFileSystem> {
        self.dir.vfat.clone()
    }

    fn set_file_size(&mut self, size: u32) -> io::Result<()> {
        assert!(!self.metadata.is_dir());
        self.dir.set_file_size(self.regular_entry_index, size)
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
            regular_entry_index: self.regular_entry_index,
            ref_guard,
        }
    }
}

impl Entry for VFatEntry {
    type File = VFatFile;
    type Dir = VFatDir;
    type Metadata = VFatMetadata;

    fn name(&self) -> &str {
        &self.name
    }

    fn metadata(&self) -> &VFatMetadata {
        &self.metadata
    }

    fn open_file(&self, mode: FileOpenMode) -> io::Result<VFatFile> {
        if !self.metadata.is_dir() {
            VFatFile::from_entry(self, mode)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "not a regular file"))
        }
    }

    fn open_dir(&self) -> io::Result<VFatDir> {
        if self.metadata.is_dir() {
            Ok(VFatDir::from_entry(self))
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "not a directory"))
        }
    }

    fn is_file(&self) -> bool {
        !self.metadata.is_dir()
    }

    fn is_dir(&self) -> bool {
        self.metadata.is_dir()
    }
}
