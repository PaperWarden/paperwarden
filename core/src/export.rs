//! Explicit export of a verified output to a new file
//! (`PRODUCT_INVARIANTS.md` 8).
//!
//! Export only ever creates a file. If anything exists at the destination,
//! including the original input, it fails and leaves that file untouched. A
//! partially written export is removed.

use crate::runner::JobError;
use crate::CancellationToken;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

const CHUNK: usize = 64 * 1024;

/// Copies `source` to a new file at `destination`; returns the bytes written.
pub(crate) fn export_new(
    source: &Path,
    destination: &Path,
    cancel: &CancellationToken,
) -> Result<u64, JobError> {
    let mut reader = File::open(source).map_err(|error| JobError::Io(error.kind()))?;
    let mut writer = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(JobError::DestinationExists)
        }
        Err(error) => return Err(JobError::Io(error.kind())),
    };
    let result = copy(&mut reader, &mut writer, cancel).and_then(|written| {
        writer
            .sync_all()
            .map(|()| written)
            .map_err(|error| JobError::Io(error.kind()))
    });
    if result.is_err() {
        drop(writer);
        // Only the file this export created is removed.
        let _ = fs::remove_file(destination);
    }
    result
}

fn copy(reader: &mut File, writer: &mut File, cancel: &CancellationToken) -> Result<u64, JobError> {
    let mut buffer = vec![0_u8; CHUNK];
    let mut total: u64 = 0;
    loop {
        if cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| JobError::Io(error.kind()))?;
        if read == 0 {
            return Ok(total);
        }
        let chunk = buffer
            .get(..read)
            .ok_or(JobError::Io(io::ErrorKind::InvalidData))?;
        writer
            .write_all(chunk)
            .map_err(|error| JobError::Io(error.kind()))?;
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
    }
}
