use traits;
use vfat::{File, Dir};
use vfat::Entry;
use vfat::Shared;
use vfat::VFat;

pub struct FileSystemObject {
    vfat: Shared<VFat>,
    first_cluster: u32,
    size: u32,
    is_dir: bool,
}

impl FileSystemObject {
    pub fn from_entry(vfat: Shared<VFat>, entry: &Entry) -> Self {
        Self {
            vfat,
            first_cluster: entry.metadata.first_cluster,
            size: entry.metadata.size,
            is_dir: entry.metadata.attributes.is_dir(),
        }
    }

    pub fn root(vfat: Shared<VFat>) -> Self {
        let first_cluster = vfat.borrow().root_dir_cluster;
        Self {
            vfat,
            first_cluster,
            size: 0,
            is_dir: true,
        }
    }
}

impl traits::FileSystemObject for FileSystemObject {
    type File = File;
    type Dir = Dir;

    fn into_file(self) -> Option<File> {
        if !self.is_dir {
            Some(File::open(self.vfat, self.first_cluster, self.size, unimplemented!(), unimplemented!()))
        } else {
            None
        }
    }

    fn into_dir(self) -> Option<Dir> {
        if self.is_dir {
            Some(Dir::open(self.vfat, self.first_cluster))
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
