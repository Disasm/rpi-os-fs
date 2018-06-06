use std::fmt;

use traits::BlockDevice;
use vfat::Error;

#[repr(C, packed)]
pub struct BiosParameterBlock {
    _data: [u8; 0xb],
    _legacy_bpb: [u8; 25],
    logical_sectors_per_fat: u32,
    mirroring_flags: u16,
    version: u16,
    root_directory_cluster: u32,
    fs_information_sector_location: u16,
    backup_sector_location: u16,
    _reserved: [u8; 12],
    physical_driver_number: u8,
    flags: u8,
    extended_boot_signature: u8,
    volume_serial_number: u32,
    volume_label: [u8; 11],
    fs_type: [u8; 8],
    _data2: [u8; 420],
    signature: u16,
}

impl BiosParameterBlock {
    /// Reads the FAT32 extended BIOS parameter block from sector `sector` of
    /// device `device`.
    ///
    /// # Errors
    ///
    /// If the EBPB signature is invalid, returns an error of `BadSignature`.
    pub fn from<T: BlockDevice>(
        mut device: T,
        sector: u64
    ) -> Result<BiosParameterBlock, Error> {
        let mut buf = [0; 512];
        let size = device.read_sector(sector, &mut buf).map_err(|e| Error::Io(e))?;
        let pbp: BiosParameterBlock = unsafe { ::std::mem::transmute(buf) };
        if pbp.signature != 0xAA55 {
            return Err(Error::BadSignature)
        }
        Ok(pbp)
    }
}

impl fmt::Debug for BiosParameterBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BiosParameterBlock {{ fs_type={:?} }}", self.fs_type)
    }
}
