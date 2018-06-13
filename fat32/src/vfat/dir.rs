use std::ffi::OsStr;
use std::char::decode_utf16;
use std::borrow::Cow;
use std::io;

use traits;
use util::VecExt;
use vfat::{VFat, Shared, File, Entry};
use std::mem;
use std::io::Read;

//#[derive(Debug)]
pub struct Dir {
    vfat: Shared<VFat>,
    start_cluster: u32,
    size: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct VFatRegularDirEntry {
    file_name: [u8; 8],
    file_ext: [u8; 3],
    attributes: u8,
    _reserved: u8,
    created_time_hundredths: u8,
    created_time: u16,
    created_date: u16,
    accessed_date: u16,
    cluster_high: u16,
    modified_time: u16,
    modified_date: u16,
    cluster_low: u16,
    size: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct VFatLfnDirEntry {
    sequence_number: u8,
    name: [u16; 5],
    attributes: u8,
    entry_type: u8,
    checksum: u8,
    name2: [u16; 6],
    _always_zero: [u8; 2],
    name3: [u16; 2],
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct VFatUnknownDirEntry {
    _unknown: [u8; 11],
    attributes: u8,
    _unknown2: [u8; 20],
}

pub union VFatDirEntry {
    unknown: VFatUnknownDirEntry,
    regular: VFatRegularDirEntry,
    long_filename: VFatLfnDirEntry,
}

impl Dir {
    /// Finds the entry named `name` in `self` and returns it. Comparison is
    /// case-insensitive.
    ///
    /// # Errors
    ///
    /// If no entry with name `name` exists in `self`, an error of `NotFound` is
    /// returned.
    ///
    /// If `name` contains invalid UTF-8 characters, an error of `InvalidInput`
    /// is returned.
    pub fn find<P: AsRef<OsStr>>(&self, name: P) -> io::Result<Entry> {
        unimplemented!("Dir::find()")
    }
}

struct RawDirIterator {
    file: File,
}

impl Iterator for RawDirIterator {
    type Item = io::Result<VFatDirEntry>;

    fn next(&mut self) -> Option<io::Result<VFatDirEntry>> {
        if self.file.at_end() {
            return None;
        }
        let mut buf = [0; 32];
        match self.file.read_exact(&mut buf) {
            Ok(_) => Some(Ok(unsafe { mem::transmute(buf) })),
            Err(e) => Some(Err(e)),
        }
    }
}

pub struct DirIterator {
    inner: RawDirIterator,
}

impl Iterator for DirIterator {
    type Item = io::Result<Entry>;

    fn next(&mut self) -> Option<io::Result<Entry>> {
        unimplemented!()
    }
}

// FIXME: Implement `trait::Dir` for `Dir`.
impl traits::Dir for Dir {
    type Entry = Entry;
    type Iter = DirIterator;

    fn entries(&self) -> io::Result<DirIterator> {
        let raw_iterator = RawDirIterator {
            file: File::open(self.vfat.clone(), self.start_cluster, self.size)
        };
        Ok(DirIterator {
            inner: raw_iterator
        })
    }
}
