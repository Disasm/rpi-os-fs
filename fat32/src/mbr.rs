use std::{fmt, io};

use traits::BlockDevice;
use partition::Partition;

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct CHS {
    c: u8,
    h: u8,
    s: u8,
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct PartitionEntry {
    boot_indicator: u8,
    start_chs: CHS,
    entry_type: u8,
    end_chs: CHS,
    start_lba: u32,
    size: u32,
}

/// The master boot record (MBR).
#[repr(C, packed)]
pub struct MasterBootRecord {
    _data: [u8; 446],
    entries: [PartitionEntry; 4],
    signature: u16,
}

#[derive(Debug)]
pub enum Error {
    /// There was an I/O error while reading the MBR.
    Io(io::Error),
    /// Partiion `.0` (0-indexed) contains an invalid or unknown boot indicator.
    UnknownBootIndicator(u8),
    /// The MBR magic signature was invalid.
    BadSignature,
}

impl MasterBootRecord {
    /// Reads and returns the master boot record (MBR) from `device`.
    ///
    /// # Errors
    ///
    /// Returns `BadSignature` if the MBR contains an invalid magic signature.
    /// Returns `UnknownBootIndicator(n)` if partition `n` contains an invalid
    /// boot indicator. Returns `Io(err)` if the I/O error `err` occured while
    /// reading the MBR.
    pub fn read_from<T: BlockDevice>(device: &mut T) -> Result<MasterBootRecord, Error> {
        let mut buf = [0; 512];
        let size = device.read_sector(0, &mut buf).map_err(|e| Error::Io(e))?;
        let mbr: MasterBootRecord = unsafe { ::std::mem::transmute(buf) };
        if mbr.signature != 0xAA55 {
            return Err(Error::BadSignature)
        }
        for (i, entry) in mbr.entries.iter().enumerate() {
            if entry.boot_indicator != 0x00 && entry.boot_indicator != 0x80 {
                return Err(Error::UnknownBootIndicator(i as u8))
            }
        }
        Ok(mbr)
    }
}

pub fn get_partition<T: BlockDevice>(mut device: T, partition_number: usize) -> io::Result<Partition<T>> {
    if partition_number >= 4 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mbr = MasterBootRecord::read_from(&mut device).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    let entry = &mbr.entries[partition_number];
    if entry.entry_type == 0 {
        return Err(io::ErrorKind::NotFound.into());
    }
    let sector_start = entry.start_lba as u64;
    let sector_end = sector_start + entry.size as u64;
    Ok(Partition::new(device, sector_start..sector_end))
}

impl fmt::Debug for MasterBootRecord {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        unimplemented!("MasterBootRecord::fmt()")
    }
}
