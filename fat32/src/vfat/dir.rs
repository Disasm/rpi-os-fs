use std::io;

use vfat::{VFatFileSystem, VFatEntry};
use std::mem;
use std::io::{Read, Write, Seek, SeekFrom};
use fallible_iterator::FallibleIterator;
use traits::{Dir, Date, Time, DateTime, Entry};
use vfat::metadata::VFatMetadata;
use vfat::metadata::Attributes;
use vfat::cluster_chain::ClusterChain;
use vfat::lock_manager::LockMode;
use chrono::{Datelike, Timelike};
use std::ops::RangeInclusive;
use arc_mutex::ArcMutex;

pub struct VFatDir {
    pub(crate) vfat: ArcMutex<VFatFileSystem>,
    pub(crate) chain: ClusterChain,

    #[allow(unused)]
    entry: Option<VFatEntry>,
}

#[derive(Clone)]
pub struct SharedVFatDir(pub(crate) ArcMutex<VFatDir>);

#[derive(Debug)]
pub(crate) struct VFatSimpleDirEntry {
    name: String,
    short_name: String,
    metadata: VFatMetadata,
    entry_index_range: RangeInclusive<u64>,
}

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

fn date_to_vfat_repr(date: &Date) -> u16 {
    if date.year() < 1980 || date.year() > 2107 {
        return 0;
    }
    (((date.year() as u32 - 1980) << 9) | (date.month() << 5) | date.day()) as u16
}

fn time_to_vfat_repr(time: &Time) -> u16 {
    ((time.hour() << 11) | (time.minute() << 5) | (time.second() / 2)) as u16
}

impl VFatRegularDirEntry {
    fn from(name: &str, ext: &str, metadata: &VFatMetadata) -> Self {
        let mut file_name = [0; 8];
        file_name[..name.len()].copy_from_slice(name.as_bytes());
        let mut file_ext = [0; 3];
        file_ext[..ext.len()].copy_from_slice(ext.as_bytes());
        Self {
            file_name,
            file_ext,
            attributes: metadata.attributes.0,
            _reserved: 0,
            created_time_hundredths: 0,
            created_time: time_to_vfat_repr(&metadata.created.time()),
            created_date: date_to_vfat_repr(&metadata.created.date()),
            accessed_date: date_to_vfat_repr(&metadata.accessed),
            cluster_high: (metadata.first_cluster >> 16) as u16,
            cluster_low: metadata.first_cluster as u16,
            modified_time: time_to_vfat_repr(&metadata.modified.time()),
            modified_date: date_to_vfat_repr(&metadata.modified.date()),
            size: metadata.size,
        }
    }

    fn checksum(&self) -> u8 {
        let mut sum = 0u8;
        for b in self.file_name.iter().chain(self.file_ext.iter()) {
            sum = (sum >> 1) + ((sum & 1) << 7);  /* rotate */
            sum = sum.wrapping_add(*b);
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
    pub const SIZE: usize = ::std::mem::size_of::<Self>();

    pub fn new_free() -> Self {
        unsafe {
            let mut entry: VFatDirEntry = mem::zeroed();
            entry.unknown.first_byte = 0xe5;
            entry
        }
    }

    pub fn new_eof_mark() -> Self {
        unsafe {
            mem::zeroed()
        }
    }

    pub fn is_regular(&self) -> bool {
        self.is_valid() && !self.is_lfn()
    }

    pub fn is_lfn(&self) -> bool {
        self.is_valid() && unsafe { self.unknown }.attributes == 0x0f
    }

    pub fn is_valid(&self) -> bool {
        unsafe { self.unknown }.first_byte != 0xe5
    }
}



impl VFatDir {
    pub fn open(vfat: ArcMutex<VFatFileSystem>, first_cluster: u32, entry: Option<VFatEntry>) -> Option<SharedVFatDir> {
        ClusterChain::open(vfat.clone(), first_cluster, LockMode::Write).map(|chain| {
            SharedVFatDir(ArcMutex::new(VFatDir {
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

    pub(crate) fn get_raw_entry(&mut self, index: u64) -> io::Result<Option<VFatDirEntry>> {
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

    pub(crate) fn set_raw_entry(&mut self, index: u64, entry: &VFatDirEntry) -> io::Result<()> {
        self.chain.seek(SeekFrom::Start(index * VFatDirEntry::SIZE as u64))?;

        assert_eq!(VFatDirEntry::SIZE, mem::size_of::<VFatDirEntry>());
        let buf = unsafe {
            ::std::slice::from_raw_parts(entry as *const VFatDirEntry as *const u8, VFatDirEntry::SIZE)
        };
        self.chain.write_all(buf)
    }

    pub fn remove_entry(&mut self, entry: &VFatEntry) -> io::Result<()> {
        for index in entry.dir_entry_index_range.clone() {
            self.set_raw_entry(index, &VFatDirEntry::new_free())?;
        }
        Ok(())
    }

    pub(crate) fn create_entry(&mut self, file_name: &str, metadata: &VFatMetadata) -> io::Result<VFatSimpleDirEntry> {
        if (file_name.len() >= 255) || (file_name.len() == 0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "incorrect file name length"));
        }
        if self.has_entry_with_name(file_name)? {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        let utf16_file_name: Vec<_> = file_name.encode_utf16().collect();
        let total_entry_count = (utf16_file_name.len() + 12) / 13 + 1;

        let mut free_count: u64 = 0;
        let mut index = 0;
        let mut at_end = false;
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
                free_count += 1;
                at_end = true;
                break;
            }
            index += 1;
        }
        let alloc_index = index - free_count + 1;
        let short_file_name = format!("_~{}", alloc_index);
        let regular_entry = VFatRegularDirEntry::from(&short_file_name, "", metadata);
        let lfn_entries = create_lfn_entries(file_name, regular_entry.checksum());
        assert_eq!(lfn_entries.len() + 1, total_entry_count);

        for (i, entry) in lfn_entries.iter().enumerate() {
            self.set_raw_entry(alloc_index + i as u64, entry.as_union())?;
        }
        let regular_entry_index = alloc_index + lfn_entries.len() as u64;
        self.set_raw_entry(regular_entry_index, regular_entry.as_union())?;
        if at_end {
            self.set_raw_entry(regular_entry_index + 1, &VFatDirEntry::new_eof_mark())?;
        }

        let entry = VFatSimpleDirEntry {
            name: file_name.to_string(),
            short_name: short_file_name,
            metadata: metadata.clone(),
            entry_index_range: alloc_index..=regular_entry_index,
        };
        Ok(entry)
    }

    fn next_simple_entry(&mut self, index: u64) -> io::Result<Option<VFatSimpleDirEntry>> {
        let mut raw_iterator = RawDirIterator {
            dir: self,
            raw_index: index,
        };

        if let Some((raw_index, entry)) = raw_iterator.find(|&(_, ref entry)| entry.is_valid())? {
            let (long_name, regular_entry, regular_entry_index) = if entry.is_lfn() {
                let lfn_entry = unsafe { entry.long_filename };
                if lfn_entry.sequence_number & 0x40 == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid sequence number for the first LFN entry"));
                }
                let lfn_entries_count = lfn_entry.sequence_number & 0x1F;

                let mut entries = vec![lfn_entry];
                for i in 1..lfn_entries_count {
                    if let Some((_, entry)) = raw_iterator.next()? {
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

                let (next_entry_index, next_entry) = raw_iterator.next()?.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "can't find regular entry after long entry"))?;
                if !next_entry.is_regular() {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "next entry is not regular"));
                }
                (long_name, next_entry, next_entry_index)
            } else {
                assert!(entry.is_regular());
                (None, entry, raw_index)
            };

            let regular_entry = unsafe { regular_entry.regular };
            let short_file_name = {
                let file_name = bytes_to_short_filename(&regular_entry.file_name)?;
                let file_ext = bytes_to_short_filename(&regular_entry.file_ext)?;
                if file_ext.len() > 0 {
                    format!("{}.{}", file_name, file_ext)
                } else {
                    file_name.to_string()
                }
            };
            let file_name = long_name.unwrap_or_else(|| short_file_name.clone());
            let metadata = VFatMetadata {
                attributes: Attributes(regular_entry.attributes),
                created: DateTime::new(decode_date(regular_entry.created_date), decode_time(regular_entry.created_time)?),
                accessed: decode_date(regular_entry.accessed_date),
                modified: DateTime::new(decode_date(regular_entry.modified_date), decode_time(regular_entry.modified_time)?),
                first_cluster: ((regular_entry.cluster_high as u32) << 16) | (regular_entry.cluster_low as u32),
                size: regular_entry.size,
            };
            let entry = VFatSimpleDirEntry {
                name: file_name,
                short_name: short_file_name,
                metadata,
                entry_index_range: (raw_index as u64)..=(regular_entry_index as u64),
            };
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    fn has_entry_with_name(&mut self, name: &str) -> io::Result<bool> {
        let mut index = 0;
        while let Some(simple_entry) = self.next_simple_entry(index)? {
            index = simple_entry.entry_index_range.end + 1;
            if &simple_entry.name == name {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn init_empty(&mut self, time: DateTime) -> io::Result<()> {
        if self.entry.is_some() {
            let dot_metadata = VFatMetadata {
                attributes: Attributes::new(true),
                created: time,
                accessed: time.date(),
                modified: time,
                first_cluster: self.chain.first_cluster,
                size: 0,
            };
            let dot_entry = VFatRegularDirEntry::from(".", "", &dot_metadata);
            self.set_raw_entry(0, &dot_entry.as_union())?;

            let parent_dir = self.entry.as_ref().unwrap().parent();
            let parent_first_cluster = parent_dir.0.lock().chain.first_cluster;
            let dotdot_metadata = VFatMetadata {
                first_cluster: parent_first_cluster,
                ..dot_metadata
            };
            let dotdot_entry = VFatRegularDirEntry::from("..", "", &dotdot_metadata);
            self.set_raw_entry(1, &dotdot_entry.as_union())?;

            self.set_raw_entry(2, &VFatDirEntry::new_eof_mark())?;
        } else {
            self.set_raw_entry(0, &VFatDirEntry::new_eof_mark())?;
        }

        Ok(())
    }
}



pub(crate) struct RawDirIterator<'a> {
    pub(crate) dir: &'a mut VFatDir,
    pub(crate) raw_index: u64,
}

impl<'a> FallibleIterator for RawDirIterator<'a> {
    type Item = (u64, VFatDirEntry);
    type Error = io::Error;

    fn next(&mut self) -> io::Result<Option<(u64, VFatDirEntry)>> {
        let current_index = self.raw_index;
        let entry = self.dir.get_raw_entry(self.raw_index)?;
        self.raw_index += 1;
        Ok(entry.map(|entry| (current_index, entry)))
    }
}

pub struct DirIterator {
    index: u64,
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

fn decode_date(raw_date: u16) -> Date {
    let year = (raw_date >> 9) + 1980;
    let month = (raw_date >> 5) & 0b1111;
    let second = raw_date & 0b11111;
    Date::from_ymd_opt(year as i32, month as u32, second as u32).unwrap_or_else(|| Date::from_ymd(1980, 1, 1))
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
        let vfat = self.dir.0.lock().vfat.clone();
        while let Some(simple_entry) = self.dir.0.lock().next_simple_entry(self.index)? {
            self.index = simple_entry.entry_index_range.end + 1;
            if simple_entry.metadata.attributes.is_volume_id() { // skip volume id
                continue;
            }
            if simple_entry.name == "." || simple_entry.name == ".." {
                continue;
            }
            let entry = self.dir.convert_entry(simple_entry, vfat);
            return Ok(Some(entry));
        }
        Ok(None)
    }
}


impl Dir for SharedVFatDir {
    type Entry = VFatEntry;
    type Iter = DirIterator;

    fn entries(&self) -> io::Result<DirIterator> {
        Ok(DirIterator {
            index: 0,
            dir: self.clone(),
        })
    }

    fn entry(&self) -> Option<VFatEntry> {
        self.0.lock().entry.as_ref().map(|e| e.clone())
    }
}

impl SharedVFatDir {
    fn convert_entry(&self, raw_entry: VFatSimpleDirEntry, vfat: ArcMutex<VFatFileSystem>) -> VFatEntry {
        let ref_guard = vfat.lock().lock_manager().lock(raw_entry.metadata.first_cluster, LockMode::Ref);
        VFatEntry {
            name: raw_entry.name,
            metadata: raw_entry.metadata,
            dir: self.clone(),
            dir_entry_index_range: raw_entry.entry_index_range,
            ref_guard,
        }
    }

    pub fn create_entry(&self, file_name: &str, metadata: &VFatMetadata) -> io::Result<VFatEntry> {
        let mut dir = self.0.lock();
        let raw_entry = dir.create_entry(file_name, metadata)?;

        Ok(self.convert_entry(raw_entry, dir.vfat.clone()))
    }
}
