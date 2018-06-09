#![feature(decl_macro, conservative_impl_trait)]
#![feature(range_contains)]
#![allow(safe_packed_borrows)]
#![feature(use_nested_groups)]

#[cfg(not(target_endian="little"))]
compile_error!("only little endian platforms supported");

#[cfg(test)]
mod tests;
mod mbr;
mod util;
mod partition;
mod cache;

pub mod vfat;
pub mod traits;

pub use mbr::*;

