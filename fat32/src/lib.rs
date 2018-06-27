#![feature(decl_macro, conservative_impl_trait)]
#![feature(range_contains)]
#![allow(safe_packed_borrows)]
#![feature(use_nested_groups)]
#![feature(dotdoteq_in_patterns)]
#![feature(inclusive_range_syntax)]

#[cfg(not(target_endian="little"))]
compile_error!("only little endian platforms supported");

#[cfg(test)]
mod tests;
pub mod mbr;
mod partition;
pub mod cache;

pub mod vfat;
pub mod traits;

pub use mbr::*;

pub extern crate chrono;
pub extern crate fallible_iterator;
