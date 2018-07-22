use vfat::{VFatFile, VFatDir};
use vfat::VFatEntry;
use vfat::ArcMutex;
use vfat::VFatFileSystem;
use traits::FileSystemObject;

pub struct VFatObject {
    vfat: ArcMutex<VFatFileSystem>,
    first_cluster: u32,
    size: u32,
    is_dir: bool,
}

impl VFatObject {
    pub fn from_entry(vfat: ArcMutex<VFatFileSystem>, entry: &VFatEntry) -> Self {
        Self {
            vfat,
            first_cluster: entry.metadata.first_cluster,
            size: entry.metadata.size,
            is_dir: entry.metadata.attributes.is_dir(),
        }
    }

    pub fn root(vfat: ArcMutex<VFatFileSystem>) -> Self {
        let first_cluster = vfat.lock().root_dir_cluster;
        Self {
            vfat,
            first_cluster,
            size: 0,
            is_dir: true,
        }
    }
}

impl FileSystemObject for VFatObject {
    type File = VFatFile;
    type Dir = VFatDir;

    fn into_file(self) -> Option<VFatFile> {
        if !self.is_dir {
            Some(VFatFile::open(self.vfat, self.first_cluster, self.size))
        } else {
            None
        }
    }

    fn into_dir(self) -> Option<VFatDir> {
        if self.is_dir {
            Some(VFatDir::open(self.vfat, self.first_cluster))
        } else {
            None
        }
    }

    fn is_file(&self) -> bool {
        !self.is_dir
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }
}
