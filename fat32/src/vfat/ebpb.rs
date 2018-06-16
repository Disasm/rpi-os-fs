use std::fmt;

use traits::BlockDevice;
use vfat::Error;

#[repr(C, packed)]
pub struct BiosParameterBlock {
    pub _data: [u8; 0xb],
    // DOS 2.0 BPB
    pub bytes_per_logical_sector: u16,
    pub logical_sectors_per_cluster: u8,
    pub reserved_logical_sectors: u16,
    pub number_of_fats: u8,
    pub root_directory_entries: u16,
    pub total_logical_sectors: u16,
    pub media_descriptor: u8,
    pub _logical_sectors_per_fat_legacy: u16,

    // DOS 3.31 BPB
    pub physical_sectors_per_track: u16,
    pub number_of_heads: u16,
    pub hidden_sectors: u32,
    pub large_total_logical_sectors: u32,

    // DOS 7.1 EBPB
    pub logical_sectors_per_fat: u32,
    pub mirroring_flags: u16,
    pub version: u16,
    pub root_directory_cluster: u32,
    pub fs_information_sector_location: u16,
    pub backup_sector_location: u16,
    pub _reserved: [u8; 12],
    pub physical_driver_number: u8,
    pub flags: u8,
    pub extended_boot_signature: u8,
    pub volume_serial_number: u32,
    pub volume_label: [u8; 11],
    pub fs_type: [u8; 8],
    pub _data2: [u8; 420],
    pub signature: u16,
}

impl BiosParameterBlock {
    /// Reads the FAT32 extended BIOS parameter block from sector `sector` of
    /// device `device`.
    ///
    /// # Errors
    ///
    /// If the EBPB signature is invalid, returns an error of `BadSignature`.
    pub fn read_from<T: BlockDevice>(
        device: &T
    ) -> Result<BiosParameterBlock, Error> {
        let mut buf = [0; 512];
        device.read_sector(0, &mut buf).map_err(|e| Error::Io(e))?;
        let bpb: BiosParameterBlock = unsafe { ::std::mem::transmute(buf) };
        if bpb.signature != 0xAA55 {
            return Err(Error::BadSignature)
        }
        Ok(bpb)
    }
}

impl fmt::Debug for BiosParameterBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BiosParameterBlock {{ fs_type={:?} }}", self.fs_type)
    }
}
