use anyhow::Result;
use serde::Serialize;
use std::io::{self, Write};

pub const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn check(value: &impl Serialize) -> Result<()> {
    serialized_bytes(value, MAX_METADATA_BYTES).map(|_| ())
}

pub(crate) fn serialized_bytes(value: &impl Serialize, limit: usize) -> Result<usize> {
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(bytes.len())
                .ok_or_else(|| io::Error::other("Metadata size overflow"))?;
            if self.bytes > self.limit {
                return Err(io::Error::other(
                    "Serialized metadata exceeds its byte budget",
                ));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, limit };
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.bytes)
}
