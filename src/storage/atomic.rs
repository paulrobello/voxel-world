//! Atomic write helper for small sidecar files (STOR-006).
//!
//! Sidecars like `level.dat`, `models.dat`, `door_pairs.dat`, fluid sources and
//! stencil state are load-bearing for world load. A naive `File::create` +
//! `write_all` leaves a truncated/corrupt file if the process dies mid-write.
//! These helpers write to a sibling `<name>.tmp`, fsync, then rename — rename is
//! atomic on the same filesystem, so a crash leaves either the old file or the
//! fully-written new file, never a half-written one.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Atomically write `bytes` to `path`.
///
/// Writes to `<filename>.tmp` in the same directory, fsyncs it, then renames it
/// over `path`. The temp file lives in the same directory as the destination so
/// the rename is guaranteed to stay on the same filesystem and remain atomic. A
/// stale `.tmp` left over from a prior crashed run is overwritten cleanly, since
/// the temp file is (re)created with truncation.
pub fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_path_for(path);
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Build the sibling temp path `<filename>.tmp` for `path`.
fn tmp_path_for(path: &Path) -> PathBuf {
    let tmp_name = match path.file_name() {
        Some(name) => format!("{}.tmp", name.to_string_lossy()),
        None => return path.to_path_buf(),
    };
    path.with_file_name(tmp_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_writes_all_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("level.dat");
        let payload = br#"{"seed":42,"spawn_pos":[0.0,64.0,0.0]}"#;

        atomic_write_bytes(&path, payload).unwrap();

        let read_back = std::fs::read(&path).unwrap();
        assert_eq!(read_back, payload);
    }

    #[test]
    fn no_leftover_tmp_after_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("models.dat");

        atomic_write_bytes(&path, b"postcard-bytes").unwrap();

        let tmp = tmp_path_for(&path);
        assert!(!tmp.exists(), "temp file must be gone after rename");
    }

    #[test]
    fn stale_tmp_is_overwritten_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("door_pairs.dat");
        let tmp = tmp_path_for(&path);

        // Simulate a stale temp file left behind by a prior crashed run.
        std::fs::write(&tmp, b"stale-partial-write").unwrap();
        assert!(tmp.exists());

        atomic_write_bytes(&path, b"fresh-bytes").unwrap();

        // Final file holds the fresh write, not the stale partial.
        assert_eq!(std::fs::read(&path).unwrap(), b"fresh-bytes");
        // And the temp file is cleaned up.
        assert!(!tmp.exists());
    }

    #[test]
    fn overwrite_replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("level.dat");

        atomic_write_bytes(&path, b"old").unwrap();
        atomic_write_bytes(&path, b"new-contents").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new-contents");
        assert!(!tmp_path_for(&path).exists());
    }
}
