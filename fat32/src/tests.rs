extern crate rand;

use std::io::prelude::*;
use std::io::Cursor;
use std::path::Path;

use vfat::{Shared, VFat, BiosParameterBlock};
use mbr::{MasterBootRecord, CHS, PartitionEntry, get_partition};
use traits::*;
use fallible_iterator::FallibleIterator;
use chrono::{Datelike, Timelike};

mod mock {
    use std::io::{Read, Write, Seek, Result, SeekFrom};
    pub trait MockBlockDevice : Read + Write + Seek + Send {    }

    impl<T: MockBlockDevice> ::traits::BlockDevice for T {
        fn read_sector(&mut self, n: u64, buf: &mut [u8]) -> Result<usize> {
            let sector_size = self.sector_size();
            let to_read = ::std::cmp::min(sector_size as usize, buf.len());
            self.seek(SeekFrom::Start(n * sector_size))?;
            self.read_exact(&mut buf[..to_read])?;
            Ok(to_read)
        }

        fn write_sector(&mut self, n: u64, buf: &[u8]) -> Result<usize> {
            let sector_size = self.sector_size();
            let to_write = ::std::cmp::min(sector_size as usize, buf.len());
            self.seek(SeekFrom::Start(n * sector_size))?;
            self.write_all(&buf[..to_write])?;
            Ok(to_write)
        }
    }

    impl<'a> MockBlockDevice for ::std::io::Cursor<&'a mut [u8]> { }
    impl MockBlockDevice for ::std::io::Cursor<Vec<u8>> { }
    impl MockBlockDevice for ::std::io::Cursor<Box<[u8]>> { }
    impl MockBlockDevice for ::std::fs::File { }
}

macro assert_size_eq($T:ty, $size:expr) {
    assert_eq!(::std::mem::size_of::<$T>(), $size,
        "'{}' does not have the expected size of {}", stringify!($T), $size);
}

macro assert_matches($e:expr, $variant:pat $(if $($cond:tt)*)*) {
    match $e {
        $variant $(if $($cond)*)* => {  },
        o => panic!("expected '{}' but found '{:?}'", stringify!($variant), o)
    }
}

fn resource(name: &str) -> ::std::fs::File {
    let path = format!("{}/../files/resources/{}", env!("CARGO_MANIFEST_DIR"), name);
    match ::std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("\nfailed to find assignment 2 resource '{}': {}\n\
                       => perhaps you need to run 'make fetch'?", name, e);
            panic!("missing resource");
        }
    }
}

fn assert_hash_eq(name: &str, actual: &str, expected: &str) {
    let actual = actual.trim();
    let expected = expected.trim();
    if actual != expected {
        eprintln!("\nFile system hash failed for {}!\n", name);
        eprintln!("--------------- EXPECTED ---------------");
        eprintln!("{}", expected);
        eprintln!("---------------- ACTUAL ----------------");
        eprintln!("{}", actual);
        eprintln!("---------------- END ----------------");
        panic!("hash mismatch")
    }
}

fn hash_for(name: &str) -> String {
    let mut file = resource(&format!("hashes/{}", name));
    let mut string = String::new();
    file.read_to_string(&mut string).expect("read hash to string");
    string
}

fn vfat_from_resource(name: &str) -> Shared<VFat> {
    VFat::from(get_partition(resource(name), 0).expect("get_partition failed")).expect("failed to initialize VFAT from image")
}

#[test]
fn check_mbr_size() {
    assert_size_eq!(MasterBootRecord, 512);
    assert_size_eq!(PartitionEntry, 16);
    assert_size_eq!(CHS, 3);
}

#[test]
fn check_mbr_signature() {
    let mut data = [0u8; 512];
    let e = MasterBootRecord::read_from(&mut Cursor::new(&mut data[..])).unwrap_err();
    assert_matches!(e, ::mbr::Error::BadSignature);

    data[510..].copy_from_slice(&[0x55, 0xAA]);
    MasterBootRecord::read_from(&mut Cursor::new(&mut data[..])).unwrap();
}

#[test]
fn check_mbr_boot_indicator() {
    let mut data = [0u8; 512];
    data[510..].copy_from_slice(&[0x55, 0xAA]);

    for i in 0..4usize {
        data[446 + (i.saturating_sub(1) * 16)] = 0;
        data[446 + (i * 16)] = 0xFF;
        let e = MasterBootRecord::read_from(&mut Cursor::new(&mut data[..])).unwrap_err();
        assert_matches!(e, ::mbr::Error::UnknownBootIndicator(p) if p == i as u8);
    }

    data[446 + (3 * 16)] = 0;
    MasterBootRecord::read_from(&mut Cursor::new(&mut data[..])).unwrap();
}

#[test]
fn test_mbr() {
    let mut mbr = resource("mbr.img");
    let mut data = [0u8; 512];
    mbr.read_exact(&mut data).expect("read resource data");
    let mbr = MasterBootRecord::read_from(&mut Cursor::new(&mut data[..])).expect("valid MBR");
    assert_eq!(mbr.entries[0].entry_type, 0x0b);
    assert_eq!(mbr.entries[1].entry_type, 0x00);
    assert_eq!(mbr.entries[2].entry_type, 0x00);
    assert_eq!(mbr.entries[3].entry_type, 0x00);
    let entry = &mbr.entries[0];
    assert_eq!(entry.start_lba, 1);
    assert_eq!(entry.size, 393215);
}

#[test]
fn check_ebpb_size() {
    assert_size_eq!(BiosParameterBlock, 512);
}

#[test]
fn check_ebpb_signature() {
    let mut data = [0u8; 1024];
    data[510..512].copy_from_slice(&[0x55, 0xAA]);

    let e = BiosParameterBlock::read_from(&mut Cursor::new(&mut data[512..])).unwrap_err();
    assert_matches!(e, ::vfat::Error::BadSignature);

    BiosParameterBlock::read_from(&mut Cursor::new(&mut data[..])).unwrap();
}

#[test]
fn test_ebpb() {
    let mut ebpb1 = resource("ebpb1.img");
    let mut ebpb2 = resource("ebpb2.img");

    let mut data = [0u8; 1024];
    ebpb1.read_exact(&mut data[..512]).expect("read resource data");
    ebpb2.read_exact(&mut data[512..]).expect("read resource data");

    BiosParameterBlock::read_from(&mut Cursor::new(&mut data[..])).expect("valid EBPB");
    BiosParameterBlock::read_from(&mut Cursor::new(&mut data[512..])).expect("valid EBPB");
}

#[test]
fn check_entry_sizes() {
    assert_size_eq!(::vfat::dir::VFatRegularDirEntry, 32);
    assert_size_eq!(::vfat::dir::VFatUnknownDirEntry, 32);
    assert_size_eq!(::vfat::dir::VFatLfnDirEntry, 32);
    assert_size_eq!(::vfat::dir::VFatDirEntry, 32);
}

#[test]
fn test_vfat_init() {
    vfat_from_resource("mock1.fat32.img");
    vfat_from_resource("mock2.fat32.img");
    vfat_from_resource("mock3.fat32.img");
    vfat_from_resource("mock4.fat32.img");
}

fn hash_entry<T: Entry>(hash: &mut String, entry: &T) -> ::std::fmt::Result {
    use std::fmt::Write;

    fn write_bool(to: &mut String, b: bool, c: char) -> ::std::fmt::Result {
        if b { write!(to, "{}", c) } else { write!(to, "-") }
    }

    fn write_timestamp(to: &mut String, ts: DateTime) -> ::std::fmt::Result {
        write!(to, "{:02}/{:02}/{} {:02}:{:02}:{:02} ",
               ts.month(), ts.day(), ts.year(), ts.hour(), ts.minute(), ts.second())
    }

    write_bool(hash, entry.is_dir(), 'd')?;
    write_bool(hash, entry.is_file(), 'f')?;
    write_bool(hash, entry.metadata().is_read_only(), 'r')?;
    write_bool(hash, entry.metadata().is_hidden(), 'h')?;
    write!(hash, "\t")?;

    write_timestamp(hash, entry.metadata().created())?;
    write_timestamp(hash, entry.metadata().modified())?;
    write_timestamp(hash, entry.metadata().accessed())?;
    write!(hash, "\t")?;

    write!(hash, "{}", entry.name())?;
    Ok(())
}

fn hash_dir<T: Dir>(
    hash: &mut String, dir: T
) -> Result<Vec<T::Entry>, ::std::fmt::Error> {
    let entries_iter = dir.entries()
        .expect("entries interator");
    let mut entries = entries_iter.collect::<Vec<_>>().unwrap();

    entries.sort_by(|a, b| a.name().cmp(b.name()));
    for (i, entry) in entries.iter().enumerate() {
        if i != 0 { hash.push('\n'); }
        hash_entry(hash, entry)?;
    }

    Ok(entries)
}

fn hash_dir_from<P: AsRef<Path>>(vfat: Shared<VFat>, path: P) -> String {
    let mut hash = String::new();
    hash_dir(&mut hash, vfat.open_dir(path).expect("directory exists")).unwrap();
    hash
}

#[test]
fn test_root_entries() {
    let hash = hash_dir_from(vfat_from_resource("mock1.fat32.img"), "/");
    assert_hash_eq("mock 1 root directory", &hash, &hash_for("root-entries-1"));

    let hash = hash_dir_from(vfat_from_resource("mock2.fat32.img"), "/");
    assert_hash_eq("mock 2 root directory", &hash, &hash_for("root-entries-2"));

    let hash = hash_dir_from(vfat_from_resource("mock3.fat32.img"), "/");
    assert_hash_eq("mock 3 root directory", &hash, &hash_for("root-entries-3"));

    let hash = hash_dir_from(vfat_from_resource("mock4.fat32.img"), "/");
    assert_hash_eq("mock 4 root directory", &hash, &hash_for("root-entries-4"));
}

fn hash_dir_recursive<P: AsRef<Path>>(
    hash: &mut String,
    vfat: Shared<VFat>,
    path: P
) -> ::std::fmt::Result {
    use std::fmt::Write;

    let path = path.as_ref();
    let dir = vfat.open_dir(path).expect("directory");

    write!(hash, "{}\n", path.display())?;
    let entries = hash_dir(hash, dir)?;
    if entries.iter().any(|e| Entry::is_dir(e)) {
        hash.push_str("\n\n");
    }

    for entry in entries {
        if Entry::is_dir(&entry) && entry.name() != "." && entry.name() != ".." {
            let path = path.join(entry.name());
            hash_dir_recursive(hash, vfat.clone(), path)?;
        }
    }

    Ok(())
}

fn hash_dir_recursive_from<P: AsRef<Path>>(vfat: Shared<VFat>, path: P) -> String {
    let mut hash = String::new();
    hash_dir_recursive(&mut hash, vfat, path).unwrap();
    hash
}

#[test]
fn test_all_dir_entries() {
    let hash = hash_dir_recursive_from(vfat_from_resource("mock1.fat32.img"), "/");
    assert_hash_eq("mock 1 all dir entries", &hash, &hash_for("all-entries-1"));

    let hash = hash_dir_recursive_from(vfat_from_resource("mock2.fat32.img"), "/");
    assert_hash_eq("mock 2 all dir entries", &hash, &hash_for("all-entries-2"));

    let hash = hash_dir_recursive_from(vfat_from_resource("mock3.fat32.img"), "/");
    assert_hash_eq("mock 3 all dir entries", &hash, &hash_for("all-entries-3"));

    let hash = hash_dir_recursive_from(vfat_from_resource("mock4.fat32.img"), "/");
    assert_hash_eq("mock 4 all dir entries", &hash, &hash_for("all-entries-4"));
}

fn hash_file<T: File>(hash: &mut String, mut file: T) -> ::std::fmt::Result {
    use std::fmt::Write;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    use tests::rand::distributions::{Sample, Range};

    let mut rng = rand::thread_rng();
    let mut range = Range::new(128, 8192);
    let mut hasher = DefaultHasher::new();

    let mut bytes_read = 0;
    loop {
        let mut buffer = vec![0; range.sample(&mut rng)];
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                hasher.write(&buffer[..n]);
                bytes_read += n as u64;
            }
            Err(e) => panic!("failed to read file: {:?}", e)
        }
    }

    assert_eq!(bytes_read, file.size(),
        "expected to read {} bytes (file size) but read {}", file.size(), bytes_read);

    write!(hash, "{}", hasher.finish())
}

fn hash_files_recursive<P: AsRef<Path>>(
    hash: &mut String,
    vfat: Shared<VFat>,
    path: P
) -> ::std::fmt::Result {
    let path = path.as_ref();

    let mut entries = vfat.open_dir(path)
        .expect("directory").entries()
        .expect("entries interator")
        .collect::<Vec<_>>().unwrap();

    entries.sort_by(|a, b| a.name().cmp(b.name()));
    for entry in entries {
        let path = path.join(entry.name());
        if entry.is_file() && !entry.name().starts_with(".BC.T") {
            use std::fmt::Write;
            let file = ::vfat::file_system_object::FileSystemObject::from_entry(vfat.clone(), entry).into_file().unwrap();
            if file.size() < (1 << 20) {
                write!(hash, "{}: ", path.display())?;
                hash_file(hash, file).expect("successful hash");
                hash.push('\n');
            }
        } else if Entry::is_dir(&entry) && entry.name() != "." && entry.name() != ".." {
            hash_files_recursive(hash, vfat.clone(), path)?;
        }
    }

    Ok(())
}

fn hash_files_recursive_from<P: AsRef<Path>>(vfat: Shared<VFat>, path: P) -> String {
    let mut hash = String::new();
    hash_files_recursive(&mut hash, vfat, path).unwrap();
    hash
}

#[test]
fn test_mock1_files_recursive() {
    let hash = hash_files_recursive_from(vfat_from_resource("mock1.fat32.img"), "/");
    assert_hash_eq("mock 1 file hashes", &hash, &hash_for("files-1"));
}

#[test]
fn test_mock2_files_recursive() {
    let hash = hash_files_recursive_from(vfat_from_resource("mock2.fat32.img"), "/");
    assert_hash_eq("mock 2 file hashes", &hash, &hash_for("files-2-3-4"));
}

#[test]
fn test_mock3_files_recursive() {
    let hash = hash_files_recursive_from(vfat_from_resource("mock3.fat32.img"), "/");
    assert_hash_eq("mock 3 file hashes", &hash, &hash_for("files-2-3-4"));
}

#[test]
fn test_mock4_files_recursive() {
    let hash = hash_files_recursive_from(vfat_from_resource("mock4.fat32.img"), "/");
    assert_hash_eq("mock 4 file hashes", &hash, &hash_for("files-2-3-4"));
}

#[test]
fn shared_fs_is_sync_send_static() {
    fn f<T: Sync + Send + 'static>() {  }
    f::<Shared<VFat>>();
}

#[test]
fn mbr_get_partition() {
    let mut device = get_partition(resource("mock1.fat32.img"), 0).unwrap();

    let mut buffer = [0; 512];
    let size = device.read_sector(0, &mut buffer).unwrap();
    assert_eq!(size, 512);

    let first16 = [0xeb, 0x58, 0x90, 0x42, 0x53, 0x44, 0x20, 0x20, 0x34, 0x2e, 0x34, 0x00, 0x02, 0x01, 0x20, 0x00];
    assert_eq!(buffer[..16], first16);
    let last16 = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x55, 0xaa];
    assert_eq!(buffer[512-16..], last16);
}

#[test]
fn block_device_read_by_offset() {
    let mut device = get_partition(resource("mock1.fat32.img"), 0).unwrap();

    let mut buffer = [0; 16];
    device.read_by_offset(0, &mut buffer).unwrap();
    let first16 = [0xeb, 0x58, 0x90, 0x42, 0x53, 0x44, 0x20, 0x20, 0x34, 0x2e, 0x34, 0x00, 0x02, 0x01, 0x20, 0x00];
    assert_eq!(buffer, first16);

    device.read_by_offset(512-16, &mut buffer).unwrap();
    let last16 = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x55, 0xaa];
    assert_eq!(buffer, last16);

    device.read_by_offset(512-8, &mut buffer).unwrap();
    let bytes = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x55, 0xaa, 0x52, 0x52, 0x61, 0x41, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(buffer, bytes);
}

#[test]
fn vfat_fields() {
    let vfat = vfat_from_resource("mock1.fat32.img");
    let mut vfat = vfat.borrow_mut();
    assert_eq!(vfat.device.sector_size(), 512);
    assert_eq!(vfat.bytes_per_sector, 512);
    assert_eq!(vfat.sectors_per_cluster, 1);
    assert_eq!(vfat.sectors_per_fat, 3025);
    assert_eq!(vfat.fat_start_sector, 32);
    assert_eq!(vfat.data_start_sector, 6082);
    assert_eq!(vfat.root_dir_cluster, 2);
    assert_eq!(vfat.cluster_size_bytes(), 512);

    let mut buffer = [0; 16];
    vfat.read_cluster(2, 0, &mut buffer).unwrap();
    let first16 = [0x43, 0x53, 0x31, 0x34, 0x30, 0x45, 0x20, 0x20, 0x20, 0x20, 0x20, 0x28, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(buffer, first16);

    vfat.read_cluster(3, 0x11, &mut buffer).unwrap();
    let bytes = [0x4c, 0x5a, 0x4c, 0x00, 0x00, 0x4e, 0x01, 0x5a, 0x4c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2e];
    assert_eq!(buffer, bytes);

    let entry = vfat.fat_entry(2).unwrap();
    assert_eq!(entry.status(), ::vfat::fat::Status::Eoc(0xFFFFFFF));

    let entry = vfat.fat_entry(5).unwrap();
    assert_eq!(entry.status(), ::vfat::fat::Status::Data(6));
}

#[test]
fn vfat_file() {
    let vfat = vfat_from_resource("mock1.fat32.img");
    let mut file = ::vfat::File::open(vfat, 2, 512);

    let mut buffer = [0; 16];
    file.read_exact(&mut buffer).unwrap();
    let bytes = [0x43, 0x53, 0x31, 0x34, 0x30, 0x45, 0x20, 0x20, 0x20, 0x20, 0x20, 0x28, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(buffer, bytes);
}

#[test]
fn vfat_file2() {
    let vfat = vfat_from_resource("mock1.fat32.img");
    let mut file = ::vfat::File::open(vfat, 2, 512);

    let mut buffer = [0; 4];
    let bytes = [0x43, 0x53, 0x31, 0x34, 0x30, 0x45, 0x20, 0x20, 0x20, 0x20, 0x20, 0x28, 0x00, 0x00, 0x00, 0x00];
    file.read_exact(&mut buffer).unwrap();
    assert_eq!(buffer, bytes[0..4]);
    file.read_exact(&mut buffer).unwrap();
    assert_eq!(buffer, bytes[4..8]);
    file.read_exact(&mut buffer).unwrap();
    assert_eq!(buffer, bytes[8..12]);
    file.read_exact(&mut buffer).unwrap();
    assert_eq!(buffer, bytes[12..16]);
}

#[test]
fn vfat_cluster_chain1() {
    let vfat = vfat_from_resource("mock1.fat32.img");
    let mut chain = ::vfat::cluster_chain::ClusterChain::open(vfat, 2);

    let mut buffer = [0; 512];
    chain.read_exact(&mut buffer).unwrap();
    assert_eq!(chain.read(&mut buffer).unwrap(), 0);

    let bytes = [0x43, 0x53, 0x31, 0x34, 0x30, 0x45, 0x20, 0x20, 0x20, 0x20, 0x20, 0x28, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(buffer[..16], bytes);
}

#[test]
fn vfat_cluster_chain2() {
    let vfat = vfat_from_resource("mock1.fat32.img");
    let mut chain = ::vfat::cluster_chain::ClusterChain::open(vfat, 2);

    let mut buffer = [0; 256];
    chain.read_exact(&mut buffer).unwrap();
    let bytes = [0x43, 0x53, 0x31, 0x34, 0x30, 0x45, 0x20, 0x20, 0x20, 0x20, 0x20, 0x28, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(buffer[..16], bytes);

    chain.read_exact(&mut buffer).unwrap();
    let bytes = [0; 16];
    assert_eq!(buffer[..16], bytes);

    assert_eq!(chain.read(&mut buffer).unwrap(), 0);
}

#[test]
fn vfat_cluster_chain3() {
    let vfat = vfat_from_resource("mock1.fat32.img");
    let mut chain = ::vfat::cluster_chain::ClusterChain::open(vfat, 2);

    let mut buffer = [0; 500];
    chain.read_exact(&mut buffer).unwrap();
    let bytes = [0x43, 0x53, 0x31, 0x34, 0x30, 0x45, 0x20, 0x20, 0x20, 0x20, 0x20, 0x28, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(buffer[..16], bytes);

    let mut buffer = [0; 50];
    let size = chain.read(&mut buffer).unwrap();
    assert_eq!(size, 12);
    let bytes = [0; 12];
    assert_eq!(buffer[..12], bytes);
}

#[test]
fn vfat_cluster_chain4() {
    let vfat = vfat_from_resource("mock1.fat32.img");
    let mut chain = ::vfat::cluster_chain::ClusterChain::open(vfat, 2);

    let mut buffer = [0; 500];
    chain.read_exact(&mut buffer).unwrap();

    let mut buffer = [0; 50];
    chain.read_exact(&mut buffer).unwrap_err();
}

#[test]
fn vfat_cluster_chain5() {
    let vfat = vfat_from_resource("mock1.fat32.img");
    let mut chain = ::vfat::cluster_chain::ClusterChain::open(vfat, 5);

    let mut buffer = [0; 600];
    chain.read_exact(&mut buffer).unwrap();

    let bytes = [0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x35, 0x0d, 0x0a, 0x25, 0xb5, 0xb5, 0xb5, 0xb5, 0x0d];
    assert_eq!(buffer[..16], bytes);

    let bytes = [0x38, 0x20, 0x30, 0x20, 0x52, 0x20, 0x31, 0x36, 0x30, 0x20, 0x30, 0x20, 0x52, 0x20, 0x31, 0x36];
    assert_eq!(buffer[512..512+16], bytes);
}
