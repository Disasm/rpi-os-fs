pub type Date = ::chrono::NaiveDate;
pub type Time = ::chrono::NaiveTime;
pub type DateTime = ::chrono::NaiveDateTime;

/// Trait for directory entry metadata.
pub trait Metadata: Sized {
    fn is_dir(&self) -> bool;

    /// Whether the associated entry is read only.
    fn is_read_only(&self) -> bool;

    /// Whether the entry should be "hidden" from directory traversals.
    fn is_hidden(&self) -> bool;

    /// The timestamp when the entry was created.
    fn created(&self) -> DateTime;

    /// The timestamp for the entry's last access.
    fn accessed(&self) -> DateTime;

    /// The timestamp for the entry's last modification.
    fn modified(&self) -> DateTime;
}

