//! Performance Evidence Probe core library.
//!
//! The first module deliberately covers the crash-readability boundary shared by
//! every evidence artifact: only an incomplete record at physical EOF is
//! recoverable; corruption anywhere earlier invalidates the evidence stream.

pub mod contract;
pub mod evidence;
pub mod ndjson;
pub mod runtime;
pub mod summary;
