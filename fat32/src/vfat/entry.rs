use traits;
use vfat::metadata::Metadata;

#[derive(Debug)]
pub struct Entry {
    pub(crate) name: String,
    pub(crate) metadata: Metadata,
}

impl traits::Entry for Entry {
    type Metadata = Metadata;

    fn name(&self) -> &str {
        &self.name
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}
