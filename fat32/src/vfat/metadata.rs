use traits::{Date, DateTime, Metadata};

/// File attributes as represented in FAT32 on-disk structures.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct Attributes(pub(crate) u8);

impl Attributes {
    pub fn is_read_only(&self) -> bool {
        (self.0 & 0x01) != 0
    }

    pub fn is_hidden(&self) -> bool {
        (self.0 & 0x02) != 0
    }

    pub fn is_dir(&self) -> bool {
        (self.0 & 0x10) != 0
    }
}

/// Metadata for a directory entry.
#[derive(Debug, Clone)]
pub struct VFatMetadata {
    pub(crate) attributes: Attributes,
    pub(crate) created: DateTime,
    pub(crate) accessed: Date,
    pub(crate) modified: DateTime,
    pub(crate) first_cluster: u32,
    pub(crate) size: u32,
}

impl Metadata for VFatMetadata {
    fn is_dir(&self) -> bool {
        self.attributes.is_dir()
    }

    fn is_read_only(&self) -> bool {
        self.attributes.is_read_only()
    }

    fn is_hidden(&self) -> bool {
        self.attributes.is_hidden()
    }

    fn created(&self) -> DateTime {
        self.created
    }

    fn accessed(&self) -> DateTime {
        self.accessed.and_hms(0, 0, 0)
    }

    fn modified(&self) -> DateTime {
        self.modified
    }
}
