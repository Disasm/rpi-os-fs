use std::io;
use std::path::Path;

use traits::Metadata;
use fallible_iterator::FallibleIterator;
use std::ffi::OsStr;

/// Trait implemented by files in the file system.
pub trait File: io::Read + io::Write + io::Seek + Sized {
    /// Returns the size of the file in bytes.
    fn size(&self) -> u64;
}

/// Trait implemented by directories in a file system.
pub trait Dir: Sized {
    /// The type of entry stored in this directory.
    type Entry: Entry;

    /// An type that is an iterator over the entries in this directory.
    type Iter: FallibleIterator<Item = Self::Entry, Error=io::Error>;

    /// Returns an interator over the entries in this directory.
    fn entries(&self) -> io::Result<Self::Iter>;

    /// Finds the entry named `name` in `self` and returns it. Comparison is
    /// case-sensitive.
    ///
    /// # Errors
    ///
    /// If no entry with name `name` exists in `self`, an error of `NotFound` is
    /// returned.
    ///
    /// If `name` contains invalid UTF-8 characters, an error of `InvalidInput`
    /// is returned.
    fn find<P: AsRef<OsStr>>(&self, name: P) -> io::Result<Self::Entry> {
        if let Some(name) = name.as_ref().to_str() {
            self.entries()?.find(|entry| entry.name() == name)?.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }

    fn entry(&self) -> Option<Self::Entry>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOpenMode {
    Read,
    Write,
}

/// Trait implemented by directory entries in a file system.
///
/// An entry is either a `File` or a `Directory` and is associated with both
/// `Metadata` and a name.
pub trait Entry: Sized {
    type Metadata: Metadata;
    type File: File;
    type Dir: Dir;

    /// The name of the file or directory corresponding to this entry.
    fn name(&self) -> &str;

    /// The metadata associated with the entry.
    fn metadata(&self) -> &Self::Metadata;

    fn parent(&self) -> Self::Dir;

    fn path(&self) -> String {
        if let Some(parent_entry) = self.parent().entry() {
            format!("{}/{}", parent_entry.path(), self.name())
        } else {
            format!("/{}", self.name())
        }
    }

    /// Returns `true` if this entry is a file or `false` otherwise.
    fn is_file(&self) -> bool {
        !self.metadata().is_dir()
    }

    /// Returns `true` if this entry is a directory or `false` otherwise.
    fn is_dir(&self) -> bool {
        self.metadata().is_dir()
    }

    /// If `self` is a file, returns `Some` of the file. Otherwise returns
    /// `None`.
    fn open_file(&self, mode: FileOpenMode) -> io::Result<Self::File>;

    /// If `self` is a directory, returns `Some` of the directory. Otherwise
    /// returns `None`.
    fn open_dir(&self) -> io::Result<Self::Dir>;
}

/// Trait implemented by file systems.
pub trait FileSystem: Sized {
    /// The type of files in this file system.
    type File: File;

    /// The type of directories in this file system.
    type Dir: Dir<Entry = Self::Entry>;

    type Entry: Entry<File = Self::File, Dir = Self::Dir>;

    /// Opens the entry at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// If `path` is not absolute, an error kind of `InvalidInput` is returned.
    ///
    /// If any component but the last in `path` does not refer to an existing
    /// directory, an error kind of `InvalidInput` is returned.
    ///
    /// If there is no entry at `path`, an error kind of `NotFound` is returned.
    ///
    /// All other error values are implementation defined.
    fn get_entry<P: AsRef<Path>>(&self, path: P) -> io::Result<Self::Entry>;

    fn root(&self) -> io::Result<Self::Dir>;

    /// Opens the file at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// In addition to the error conditions for `open()`, this method returns an
    /// error kind of `Other` if the entry at `path` is not a regular file.
    fn open_file<P: AsRef<Path>>(&self, path: P, mode: FileOpenMode) -> io::Result<Self::File> {
        self.get_entry(path)?.open_file(mode)
    }

    /// Opens the directory at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// In addition to the error conditions for `open()`, this method returns an
    /// error kind of `Other` if the entry at `path` is not a directory.
    fn open_dir<P: AsRef<Path>>(&self, path: P) -> io::Result<Self::Dir> {
        let path = path.as_ref();
        if path.is_absolute() && path.parent().is_none() {
            self.root()
        } else {
            self.get_entry(path)?.open_dir()
        }
    }

    /// Creates a new file at `path`, opens it, and returns it.
    ///
    /// `path` must be absolute.
    ///
    /// # Errors
    ///
    /// If `path` is not absolute, an error kind of `InvalidInput` is returned.
    ///
    /// If any component but the last in `path` does not refer to an existing
    /// directory, an error kind of `InvalidInput` is returned.
    ///
    /// If an entry at `path` already exists, an error kind of `AlreadyExists`
    /// is returned.
    ///
    /// All other error values are implementation defined.
    fn create_file<P: AsRef<Path>>(&self, path: P) -> io::Result<Self::File>;

    /// Creates a new directory at `path`, opens it, and returns it. If
    /// `parents` is `true`, also creates all non-existent directories leading
    /// up to the last component in `path`.
    ///
    /// `path` must be absolute.
    ///
    /// # Errors
    ///
    /// If `path` is not absolute, an error kind of `InvalidInput` is returned.
    ///
    /// If any component but the last in `path` does not refer to an existing
    /// directory, or `parents` is `false` and there is no entry at that
    /// component, an error kind of `InvalidInput` is returned.
    ///
    /// If an entry at `path` already exists, an error kind of `AlreadyExists`
    /// is returned.
    ///
    /// All other error values are implementation defined.
    fn create_dir<P: AsRef<Path>>(&self, path: P) -> io::Result<Self::Dir>;

    /// Renames the entry at path `from` to `to`. But `from` and `to` must be
    /// absolute.
    ///
    /// # Errors
    ///
    /// If `from` or `to` are not absolute, an error kind of `InvalidInput` is
    /// returned.
    ///
    /// If an entry at `to` already exists, an error kind of `AlreadyExists` is
    /// returned.
    ///
    /// If there is no entry at `from`, an error kind of `NotFound` is returned.
    ///
    /// All other error values are implementation defined.
    fn rename<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: P, to: Q) -> io::Result<()>;

    /// Removes the entry at `path`. If `children` is `true` and `path` is a
    /// directory, all files in that directory are recursively removed.
    ///
    /// `path` must be absolute.
    ///
    /// # Errors
    ///
    /// If `path` is not absolute, an error kind of `InvalidInput` is returned.
    ///
    /// If there is no entry at `path`, an error kind of `NotFound` is returned.
    ///
    /// If the entry at `path` is a directory and `children` is `false`, an
    /// error kind of `Other` is returned.
    ///
    /// All other error values are implementation defined.
    fn remove<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let entry = self.get_entry(path)?;
        self.remove_entry(entry)
    }

    fn remove_entry(&self, entry: Self::Entry) -> io::Result<()>;

    fn remove_dir_recursively(&self, dir: Self::Dir) -> io::Result<()> {
        if dir.entry().is_none() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "can't remove root dir"));
        }
        {
            let mut iterator = dir.entries()?;
            while let Some(entry) = iterator.next()? {
                if entry.is_dir() {
                    let dir = entry.open_dir()?;
                    drop(entry);
                    self.remove_dir_recursively(dir)?;
                } else {
                    self.remove_entry(entry)?;
                }
            }
        }
        let entry = dir.entry().unwrap();
        drop(dir);
        self.remove_entry(entry)?;
        Ok(())
    }
}
