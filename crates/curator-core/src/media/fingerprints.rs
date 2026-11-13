//! Content hashing (BLAKE3) for exact-duplicate detection and copy verification.

use crate::core::errors::{CuratorError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub fn hash_file(path: &Path, cancel: &CancellationToken) -> Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.is_cancelled() {
            return Err(CuratorError::Cancelled);
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
