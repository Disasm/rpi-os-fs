use traits;
use vfat::{File, Dir};
use vfat::metadata::Metadata;

#[derive(Debug)]
pub struct Entry {
    name: String,
    metadata: Metadata,
}

// TODO: Implement any useful helper methods on `Entry`.

impl traits::Entry for Entry {
    type File = File;
    type Dir = Dir;
    type Metadata = Metadata;

    fn name(&self) -> &str {
        &self.name
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn into_file(self) -> Option<File> {
        unimplemented!()
    }

    fn into_dir(self) -> Option<Dir> {
        unimplemented!()
    }
}
