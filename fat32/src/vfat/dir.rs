use std::io;

use vfat::{VFatFileSystem, Shared, VFatEntry};
use std::mem;
use std::io::{Read, Write, Seek, SeekFrom};
use fallible_iterator::FallibleIterator;
use traits::{Dir, Date, Time, DateTime};
use vfat::metadata::VFatMetadata;
use vfat::metadata::Attributes;
use vfat::cluster_chain::ClusterChain;
use fallible_iterator::Enumerate;
use vfat::lock_manager::LockMode;
use std::sync::Arc;
use std::sync::Mutex;

pub struct VFatDir {
    pub(crate) vfat: Shared<VFatFileSystem>,
    pub(crate) chain: ClusterChain,

    #[allow(unused)]
    entry: Option<VFatEntry>,
}

pub type SharedVFatDir = Arc<Mutex<VFatDir>>;

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
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

impl VFatRegularDirEntry {
    fn from(name: &str, ext: &str, metadata: &VFatMetadata) -> Self {
        unimplemented!()
    }

    fn checksum(&self) -> u8 {
        let mut sum = 0u8;
        for b in self.file_name.iter().chain(self.file_ext.iter()) {
            sum = (sum >> 1) + ((sum & 1) << 7);  /* rotate */
            sum += b;
        }
        sum
    }

    fn as_union(&self) -> &VFatDirEntry {
        unsafe { mem::transmute(self) }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct VFatLfnDirEntry {
    sequence_number: u8,
    name: [u16; 5],
    attributes: u8,
    _always_zero: u8,
    checksum: u8,
    name2: [u16; 6],
    _always_zero2: [u8; 2],
    name3: [u16; 2],
}

impl VFatLfnDirEntry {
    fn as_union(&self) -> &VFatDirEntry {
        unsafe { mem::transmute(self) }
    }
}

fn create_lfn_entries(file_name: &str, checksum: u8) -> Vec<VFatLfnDirEntry> {
    assert!((file_name.len() < 255) && (file_name.len() > 0));
    let utf16_file_name: Vec<_> = file_name.encode_utf16().collect();
    utf16_file_name.chunks(13).enumerate().map(|(index, chunk)| {
        let mut part = [0xffffu16; 13];
        part[..chunk.len()].copy_from_slice(&chunk);
        if chunk.len() < part.len() {
            part[chunk.len()] = 0;
        }

        let mut entry: VFatLfnDirEntry = unsafe { mem::zeroed() };
        entry.attributes = 0x0f;
        entry.checksum = checksum;
        entry.sequence_number = index as u8 + 1;
        entry.name.copy_from_slice(&part[..5]);
        entry.name2.copy_from_slice(&part[5..11]);
        entry.name3.copy_from_slice(&part[11..]);
        entry
    }).rev().enumerate().map(|(index, mut entry)| {
        if index == 0 {
            entry.sequence_number |= 0x40;
        }
        entry
    }).collect()
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct VFatUnknownDirEntry {
    first_byte: u8,
    _unknown: [u8; 10],
    attributes: u8,
    _unknown2: [u8; 20],
}

pub union VFatDirEntry {
    unknown: VFatUnknownDirEntry,
    regular: VFatRegularDirEntry,
    long_filename: VFatLfnDirEntry,
}

impl VFatDirEntry {
    const SIZE: usize = ::std::mem::size_of::<Self>();

    fn new_free() -> Self {
        unsafe {
            let mut entry: VFatDirEntry = mem::zeroed();
            entry.unknown.first_byte = 0xe5;
            entry
        }
    }

    fn is_regular(&self) -> bool {
        self.is_valid() && !self.is_lfn()
    }

    fn is_lfn(&self) -> bool {
        self.is_valid() && unsafe { self.unknown }.attributes == 0x0f
    }

    fn is_valid(&self) -> bool {
        unsafe { self.unknown }.first_byte != 0xe5
    }
}



impl VFatDir {
    pub fn open(vfat: Shared<VFatFileSystem>, first_cluster: u32, entry: Option<VFatEntry>) -> Option<SharedVFatDir> {
        ClusterChain::open(vfat.clone(), first_cluster, LockMode::Write).map(|chain| {
            Arc::new(Mutex::new(VFatDir {
                chain,
                vfat: vfat.clone(),
                entry,
            }))
        })
    }

    pub fn set_file_size(&mut self, raw_entry_index: u64, size: u32) -> io::Result<()> {
        let mut entry = self.get_raw_entry(raw_entry_index)?.ok_or_else(|| io::Error::from(io::ErrorKind::Other))?;
        if entry.is_regular() {
            unsafe { entry.regular.size = size; }
            self.set_raw_entry(raw_entry_index, &entry)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "invalid entry type"))
        }
    }

    pub fn get_file_size(&mut self, raw_entry_index: u64) -> io::Result<u32> {
        let entry = self.get_raw_entry(raw_entry_index)?.ok_or_else(|| io::Error::from(io::ErrorKind::Other))?;
        if entry.is_regular() {
            let entry = unsafe { entry.regular };
            Ok(entry.size)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "invalid entry type"))
        }
    }

    fn get_raw_entry(&mut self, index: u64) -> io::Result<Option<VFatDirEntry>> {
        self.chain.seek(SeekFrom::Start(index * VFatDirEntry::SIZE as u64))?;
        if self.chain.at_end() {
            return Ok(None);
        }
        let mut buf = [0; VFatDirEntry::SIZE];
        self.chain.read_exact(&mut buf)?;
        let entry: VFatDirEntry = unsafe { mem::transmute(buf) };
        if unsafe { entry.unknown }.first_byte != 0x00 {
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    fn set_raw_entry(&mut self, index: u64, entry: &VFatDirEntry) -> io::Result<()> {
        self.chain.seek(SeekFrom::Start(index * VFatDirEntry::SIZE as u64))?;

        assert_eq!(VFatDirEntry::SIZE, mem::size_of::<VFatDirEntry>());
        let buf: &[u8; VFatDirEntry::SIZE] = unsafe { mem::transmute(entry) };
        self.chain.write_all(buf)
    }

    pub fn remove_entry(&mut self, entry: &VFatEntry) -> io::Result<()> {
        for index in entry.first_entry_index..=entry.regular_entry_index {
            self.set_raw_entry(index, &VFatDirEntry::new_free())?;
        }
        Ok(())
    }

    pub fn create_entry(&mut self, file_name: &str, metadata: &VFatMetadata) -> io::Result<()> {
        if (file_name.len() >= 255) || (file_name.len() == 0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "incorrent file name length"));
        }
        let utf16_file_name: Vec<_> = file_name.encode_utf16().collect();
        let total_entry_count = utf16_file_name.len() / 13 + 1;

        let mut free_count: u64 = 0;
        let mut index = 0;
        loop {
            if let Some(entry) = self.get_raw_entry(index)? {
                if entry.is_valid() {
                    free_count = 0;
                } else {
                    free_count += 1;
                }

                if free_count == total_entry_count as u64 {
                    break;
                }
            } else {
                break;
            }
            index += 1;
        }
        let alloc_index = index - free_count;
        let short_file_name = format!("_~{}", alloc_index);
        let regular_entry = VFatRegularDirEntry::from(&short_file_name, "", metadata);
        let lfn_entries = create_lfn_entries(file_name, regular_entry.checksum());
        assert_eq!(lfn_entries.len() + 1, total_entry_count);

        for (i, entry) in lfn_entries.iter().enumerate() {
            self.set_raw_entry(alloc_index + i as u64, entry.as_union())?;
        }
        self.set_raw_entry(alloc_index + lfn_entries.len() as u64, regular_entry.as_union())?;

        Ok(())
    }
}



struct RawDirIterator {
    dir: SharedVFatDir,
    raw_index: u64,
}

impl FallibleIterator for RawDirIterator {
    type Item = VFatDirEntry;
    type Error = io::Error;

    fn next(&mut self) -> io::Result<Option<VFatDirEntry>> {
        let entry = self.dir.lock().unwrap().get_raw_entry(self.raw_index)?;
        self.raw_index += 1;
        Ok(entry)
    }
}

pub struct DirIterator {
    raw_iterator: Enumerate<RawDirIterator>,
    dir: SharedVFatDir,
}

fn bytes_to_short_filename(bytes: &[u8]) -> io::Result<&str> {
    let data = if let Some(index) = bytes.iter().position(|x| *x == 0x00 || *x == 0x20) {
        &bytes[..index]
    } else {
        bytes
    };

    if !data.iter().all(|c| c.is_ascii()) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "filename contains non-ascii characters"));
    }

    ::std::str::from_utf8(data).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "can't parse filename as UTF-8"))
}

fn decode_date(raw_date: u16) -> io::Result<Date> {
    let year = (raw_date >> 9) + 1980;
    let month = (raw_date >> 5) & 0b1111;
    let second = raw_date & 0b11111;
    Date::from_ymd_opt(year as i32, month as u32, second as u32).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid date"))
}

fn decode_time(raw_time: u16) -> io::Result<Time> {
    let hour = raw_time >> 11;
    let minute = (raw_time >> 5) & 0b11_11_11;
    let second = 2 * (raw_time & 0b11111);
    Time::from_hms_opt(hour as u32, minute as u32, second as u32).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid time"))
}

impl FallibleIterator for DirIterator {
    type Item = VFatEntry;
    type Error = io::Error;

    fn next(&mut self) -> io::Result<Option<VFatEntry>> {
        if let Some((raw_index, entry)) = self.raw_iterator.find(|&(_, ref entry)| entry.is_valid())? {
            let (long_name, regular_entry, regular_entry_index) = if entry.is_lfn() {
                let lfn_entry = unsafe { entry.long_filename };
                if lfn_entry.sequence_number & 0x40 == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid sequence number for the first LFN entry"));
                }
                let lfn_entries_count = lfn_entry.sequence_number & 0x1F;

                let mut entries = vec![lfn_entry];
                for i in 1..lfn_entries_count {
                    if let Some((_, entry)) = self.raw_iterator.next()? {
                        if entry.is_lfn() {
                            let lfn_entry = unsafe { entry.long_filename };
                            let lfn_entry_index = lfn_entry.sequence_number & 0x1F;
                            if lfn_entry_index != (lfn_entries_count - i) {
                                return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid sequence number"));
                            }
                            entries.push(unsafe { entry.long_filename });
                        } else {
                            return Err(io::Error::new(io::ErrorKind::InvalidData, "unexpected LFN entry"));
                        }
                    } else {
                        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
                    }
                }

                let mut filename_buf = Vec::new();
                for entry in entries.iter().rev() {
                    filename_buf.extend_from_slice(&entry.name);
                    filename_buf.extend_from_slice(&entry.name2);
                    filename_buf.extend_from_slice(&entry.name3);
                }
                if let Some(index) = filename_buf.iter().position(|x| *x == 0x0000) {
                    filename_buf.resize(index, 0);
                }
                let long_name = String::from_utf16(&filename_buf).ok();

                let (next_entry_index, next_entry) = self.raw_iterator.next()?.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "can't find regular entry after long entry"))?;
                if !next_entry.is_regular() {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "next entry is not regular"));
                }
                (long_name, next_entry, next_entry_index)
            } else {
                assert!(entry.is_regular());
                (None, entry, raw_index)
            };

            let regular_entry = unsafe { regular_entry.regular };
            if (regular_entry.attributes & 0x08) != 0 { // skip volume id
                return self.next();
            }
            let file_name = if let Some(f) = long_name {
                f
            } else {
                let file_name = bytes_to_short_filename(&regular_entry.file_name)?;
                let file_ext = bytes_to_short_filename(&regular_entry.file_ext)?;
                if file_ext.len() > 0 {
                    format!("{}.{}", file_name, file_ext)
                } else {
                    file_name.to_string()
                }
            };

            if file_name == "." || file_name == ".." {
                return self.next();
            }

            let metadata = VFatMetadata {
                attributes: Attributes(regular_entry.attributes),
                created: DateTime::new(decode_date(regular_entry.created_date)?, decode_time(regular_entry.created_time)?),
                accessed: decode_date(regular_entry.accessed_date)?,
                modified: DateTime::new(decode_date(regular_entry.modified_date)?, decode_time(regular_entry.modified_time)?),
                first_cluster: ((regular_entry.cluster_high as u32) << 16) | (regular_entry.cluster_low as u32),
                size: regular_entry.size,
            };
            let dir = self.dir.lock().unwrap();
            let ref_guard = dir.vfat.borrow().lock_manager().lock(metadata.first_cluster, LockMode::Ref);
            let entry = VFatEntry {
                name: file_name,
                metadata,
                dir: self.dir.clone(),
                first_entry_index: raw_index as u64,
                regular_entry_index: regular_entry_index as u64,
                ref_guard,
            };
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }
}


impl Dir for SharedVFatDir {
    type Entry = VFatEntry;
    type Iter = DirIterator;

    fn entries(&self) -> io::Result<DirIterator> {
        let raw_iterator = RawDirIterator {
            dir: self.clone(),
            raw_index: 0,
        };
        Ok(DirIterator {
            raw_iterator: raw_iterator.enumerate(),
            dir: self.clone(),
        })
    }

    fn entry(&self) -> Option<VFatEntry> {
        self.lock().unwrap().entry.as_ref().map(|e| e.clone())
    }
}
