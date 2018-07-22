use traits::{Entry, Metadata};
use vfat::metadata::VFatMetadata;
use std::io;
use vfat::lock_manager::FSObjectGuard;
use vfat::VFatFile;
use vfat::lock_manager::LockMode;
use vfat::VFatFileSystem;
use traits::FileOpenMode;
use vfat::dir::SharedVFatDir;
use std::ops::RangeInclusive;
use arc_mutex::ArcMutex;

pub struct VFatEntry {
    pub(crate) name: String,
    pub(crate) metadata: VFatMetadata,
    pub(crate) dir: SharedVFatDir,
    pub(crate) dir_entry_index_range: RangeInclusive<u64>,

    #[allow(unused)]
    pub(crate) ref_guard: FSObjectGuard,
}

impl VFatEntry {
    pub(crate) fn vfat(&self) -> ArcMutex<VFatFileSystem> {
        self.dir.0.lock().vfat.clone()
    }

    pub(crate) fn set_file_size(&mut self, size: u32) -> io::Result<()> {
        assert!(!self.metadata.is_dir());
        self.dir.0.lock().set_file_size(self.dir_entry_index_range.end, size)
    }

    pub(crate) fn current_file_size(&self) -> io::Result<u32> {
        self.dir.0.lock().get_file_size(self.dir_entry_index_range.end)
    }
}

impl Clone for VFatEntry {
    fn clone(&self) -> Self {
        let vfat = self.vfat();
        let ref_guard = vfat.lock().lock_manager().lock(self.metadata.first_cluster, LockMode::Ref);
        Self {
            name: self.name.clone(),
            metadata: self.metadata.clone(),
            dir: self.dir.clone(),
            dir_entry_index_range: self.dir_entry_index_range.clone(),
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

    fn metadata(&self) -> &VFatMetadata {
        &self.metadata
    }

    fn parent(&self) -> SharedVFatDir {
        self.dir.clone()
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
            self.vfat().get_dir(self.metadata.first_cluster, Some(self.clone())).ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, "open_dir failed"))
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "not a directory"))
        }
    }
}
