use std::io;
use traits::{File, Dir, Entry, Metadata, DateTime};

/// A type that implements all of the file system traits.
#[derive(Copy, Clone)]
pub struct Dummy;

impl io::Write for Dummy {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> { panic!("Dummy") }
    fn flush(&mut self) -> io::Result<()> { panic!("Dummy") }
}

impl io::Read for Dummy {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> { panic!("Dummy") }
}

impl io::Seek for Dummy {
    fn seek(&mut self, _pos: io::SeekFrom) -> io::Result<u64> { panic!("Dummy") }
}

impl File for Dummy {
    fn sync(&mut self) -> io::Result<()> { panic!("Dummy") }
    fn size(&self) -> u64 { panic!("Dummy") }
}

/// Trait implemented by directories in a file system.
impl Dir for Dummy {
    /// The type of entry stored in this directory.
    type Entry = Dummy;
    type Iter = Dummy;

    /// Returns an interator over the entries in this directory.
    fn entries(&self) -> io::Result<Self::Iter> { panic!("Dummy") }
}

impl Iterator for Dummy {
    type Item = io::Result<Dummy>;
    fn next(&mut self) -> Option<Self::Item> { panic!("Dummy") }
}

impl Entry for Dummy {
    type File = Dummy;
    type Dir = Dummy;
    type Metadata = Dummy;

    fn name(&self) -> &str { panic!("Dummy") }
    fn metadata(&self) -> &Self::Metadata { panic!("Dummy") }
    fn into_file(self) -> Option<Self::File> { panic!("Dummy") }
    fn into_dir(self) -> Option<Self::Dir> { panic!("Dummy") }
}

impl Metadata for Dummy {
    fn is_dir(&self) -> bool { panic!("Dummy") }
    fn is_read_only(&self) -> bool { panic!("Dummy") }
    fn is_hidden(&self) -> bool { panic!("Dummy") }
    fn created(&self) -> DateTime { panic!("Dummy") }
    fn accessed(&self) -> DateTime { panic!("Dummy") }
    fn modified(&self) -> DateTime { panic!("Dummy") }
}
