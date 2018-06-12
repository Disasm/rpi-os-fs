use std::fmt;

use traits::{self, Date, DateTime};

/// File attributes as represented in FAT32 on-disk structures.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Attributes(u8);

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
pub struct Metadata {
    attributes: Attributes,
    created: DateTime,
    accessed: Date,
    modified: DateTime,
    first_cluster: u32,
}

impl traits::Metadata for Metadata {
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
