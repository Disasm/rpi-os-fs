use traits;
use vfat::metadata::Metadata;

#[derive(Debug)]
pub struct Entry {
    pub(crate) name: String,
    pub(crate) metadata: Metadata,
    pub(crate) dir_start_cluster: u32,
    pub(crate) regular_entry_index: u64,
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
