use std::fs::{File, OpenOptions};
use std::io::{Read, Result as IoResult, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::constants::WORLD_CHUNKS_Y;

/// A region file stores a 32x32 column of chunks spanning the full world height.
///
/// On-disk layout (v2):
/// ```text
/// [ location table  : CHUNKS_PER_REGION * 4 bytes (u32 BE each)        ]
/// [ timestamp table : CHUNKS_PER_REGION * 4 bytes (u32 BE each)        ]
/// [ marker          : 4-byte magic + 4-byte version (BE)               ]
/// [ generation      : 4-byte write-generation counter (BE), see STOR-003 ]
/// [ zero-padded to the next SECTOR_SIZE boundary                       ]
/// [ data sectors    : SECTOR_SIZE each ]
/// ```
/// The location + timestamp tables plus the marker occupy the leading
/// `HEADER_SIZE` bytes. v1 files (8-height, 64 KiB header, no marker) are
/// detected on open and atomically rebuilt into the v2 layout. The generation
/// counter lives in bytes that were zero padding in v2 files written by
/// STOR-001, so it is a backward-compatible additive refinement (no version
/// bump). Readers probe it to cheaply detect that a writer appended a chunk
/// since they cached the location table; see `refresh_if_stale`.
pub struct RegionFile {
    file: File,
    /// Location table: index -> (offset_in_sectors << 8) | sector_count
    locations: [u32; CHUNKS_PER_REGION],
    /// Timestamp table: index -> unix_timestamp
    timestamps: [u32; CHUNKS_PER_REGION],
    /// Write-generation counter, bumped by `write_chunk` after the location
    /// entry is durable. Readers compare their cached value against the on-disk
    /// value to decide whether to re-read the location table.
    generation: u32,
}

pub const CHUNKS_PER_REGION_SIDE: i32 = 32;
/// Region height equals the full world height. Previously this was hard-coded
/// to 8 (half of WORLD_CHUNKS_Y), which clamped chunk Y=8..15 into slots 0..7
/// and silently aliased every edit at block Y>=256 onto the chunk 256 blocks
/// below it on save. See STOR-001.
pub const REGION_HEIGHT: i32 = WORLD_CHUNKS_Y;
pub const CHUNKS_PER_REGION: usize =
    (CHUNKS_PER_REGION_SIDE * CHUNKS_PER_REGION_SIDE * REGION_HEIGHT) as usize;
pub const SECTOR_SIZE: usize = 4096;

/// Magic + version written at the end of the header so older (8-height,
/// 64 KiB-header, markerless) files can be detected and migrated instead of
/// being silently misread under the new 16-height layout.
pub const REGION_MAGIC: [u8; 4] = *b"VXRF";
pub const REGION_VERSION: u32 = 2;
/// magic + version + 4-byte generation counter (STOR-003). The generation
/// field occupies bytes that were zero padding in STOR-001 v2 files, so this
/// does not bump `REGION_VERSION`.
const REGION_MARKER_BYTES: usize =
    REGION_MAGIC.len() + std::mem::size_of::<u32>() + std::mem::size_of::<u32>();

pub const HEADER_SECTORS: usize =
    (CHUNKS_PER_REGION * 4 * 2 + REGION_MARKER_BYTES).div_ceil(SECTOR_SIZE);
pub const HEADER_SIZE: usize = HEADER_SECTORS * SECTOR_SIZE;
/// Byte offset of the marker (magic + version) within the header.
const MARKER_OFFSET: usize = CHUNKS_PER_REGION * 4 * 2;
/// Byte offset of the 4-byte BE write-generation counter, immediately after
/// the magic + version. Probed by readers to detect stale location caches.
const GENERATION_OFFSET: usize = MARKER_OFFSET + REGION_MAGIC.len() + std::mem::size_of::<u32>();

// --- Old (v1, pre-marker, 8-height) format constants, used only for migration ---
const OLD_V1_REGION_HEIGHT: i32 = 8;
const OLD_V1_CHUNKS_PER_REGION: usize =
    (CHUNKS_PER_REGION_SIDE * CHUNKS_PER_REGION_SIDE * OLD_V1_REGION_HEIGHT) as usize;
const OLD_V1_HEADER_SIZE: usize = OLD_V1_CHUNKS_PER_REGION * 4 * 2;

impl RegionFile {
    pub fn open<P: AsRef<Path>>(path: P) -> IoResult<Self> {
        let path_ref = path.as_ref();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path_ref)?;

        let file_len = file.metadata()?.len();

        // Classify the on-disk file:
        //   marker present              -> v2: load tables directly
        //   >= old v1 header, no marker -> v1 (8-height): migrate atomically
        //   anything else (short/empty) -> brand new: init as v2
        let has_marker = file_len >= HEADER_SIZE as u64 && Self::read_marker(&mut file)?;

        if has_marker {
            Self::load_v2(file)
        } else if file_len >= OLD_V1_HEADER_SIZE as u64 {
            Self::migrate_v1(path_ref.to_path_buf(), file)
        } else {
            Self::init_new(file)
        }
    }

    fn read_marker(file: &mut File) -> IoResult<bool> {
        file.seek(SeekFrom::Start(MARKER_OFFSET as u64))?;
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf)?;
        Ok(buf == REGION_MAGIC)
    }

    /// Writes magic + version at `MARKER_OFFSET` into a buffer that is at least
    /// `HEADER_SIZE` bytes long. Called on every full-header write/creation so
    /// the invariant "every new-format file on disk has the marker" holds.
    fn write_marker_into(header: &mut [u8]) {
        debug_assert!(header.len() >= MARKER_OFFSET + REGION_MARKER_BYTES);
        let magic_end = MARKER_OFFSET + REGION_MAGIC.len();
        header[MARKER_OFFSET..magic_end].copy_from_slice(&REGION_MAGIC);
        let ver_end = magic_end + std::mem::size_of::<u32>();
        header[magic_end..ver_end].copy_from_slice(&REGION_VERSION.to_be_bytes());
    }

    /// Packs `(offset_sectors, sector_count)` into a 32-bit location-table
    /// entry of the form `(offset_sectors << 8) | sector_count`.
    ///
    /// STOR-M02: the low 8 bits store the sector count, so a chunk needing more
    /// than 255 sectors would silently wrap when the previous `& 0xFF` packing
    /// dropped the high bits. The truncated count would then make `read_chunk`
    /// either read short of the real data or fail its sector-size guard,
    /// depending on the payload size. Return an explicit error instead so an
    /// oversized chunk surfaces at the write site rather than corrupting the
    /// location table.
    ///
    /// Callers must invoke this BEFORE issuing any data-sector writes so a
    /// rejection doesn't leave orphaned sectors on disk.
    fn pack_location(offset_sectors: u64, sector_count: usize) -> IoResult<u32> {
        if sector_count > u8::MAX as usize {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "chunk requires {sector_count} sectors ({} bytes payload), exceeding the \
                     8-bit sector-count field (max {} bytes); refusing to truncate the \
                     location-table entry",
                    sector_count.saturating_mul(SECTOR_SIZE),
                    u8::MAX as usize * SECTOR_SIZE,
                ),
            ));
        }
        // The high 24 bits hold the sector offset. A region file large enough to
        // overflow 24 bits (>2^24 sectors ~= 64 TiB) is far beyond any realistic
        // world and is outside STOR-M02's scope; the narrowing is left as-is.
        Ok(((offset_sectors as u32) << 8) | (sector_count as u32))
    }

    fn init_new(mut file: File) -> IoResult<Self> {
        file.set_len(HEADER_SIZE as u64)?;
        file.seek(SeekFrom::Start(0))?;
        let mut header = vec![0u8; HEADER_SIZE];
        Self::write_marker_into(&mut header);
        file.write_all(&header)?;
        // STOR-M01: `File::flush` only drains the userspace buffer; it is not a
        // durability barrier. `sync_data` forces the freshly-written header to
        // stable storage so a crash immediately after `open` can't leave a
        // half-initialised region file. `sync_all` is the documented fallback
        // on platforms whose `sync_data` returns Unsupported.
        match file.sync_data() {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::Unsupported => file.sync_all()?,
            Err(e) => return Err(e),
        }
        Ok(Self {
            file,
            locations: [0u32; CHUNKS_PER_REGION],
            timestamps: [0u32; CHUNKS_PER_REGION],
            generation: 0,
        })
    }

    fn load_v2(mut file: File) -> IoResult<Self> {
        file.seek(SeekFrom::Start(0))?;
        let mut header_buf = vec![0u8; HEADER_SIZE];
        file.read_exact(&mut header_buf)?;

        let mut locations = [0u32; CHUNKS_PER_REGION];
        let mut timestamps = [0u32; CHUNKS_PER_REGION];
        for i in 0..CHUNKS_PER_REGION {
            let off = i * 4;
            locations[i] = u32::from_be_bytes([
                header_buf[off],
                header_buf[off + 1],
                header_buf[off + 2],
                header_buf[off + 3],
            ]);
            let ts_off = CHUNKS_PER_REGION * 4 + i * 4;
            timestamps[i] = u32::from_be_bytes([
                header_buf[ts_off],
                header_buf[ts_off + 1],
                header_buf[ts_off + 2],
                header_buf[ts_off + 3],
            ]);
        }
        // Generation lives in the post-marker padding that STOR-001 v2 files
        // zero-initialised, so a missing/zero field reads as generation 0.
        let generation = u32::from_be_bytes([
            header_buf[GENERATION_OFFSET],
            header_buf[GENERATION_OFFSET + 1],
            header_buf[GENERATION_OFFSET + 2],
            header_buf[GENERATION_OFFSET + 3],
        ]);
        Ok(Self {
            file,
            locations,
            timestamps,
            generation,
        })
    }

    /// Atomically rebuilds an old (v1, 8-height, 64 KiB-header, markerless) file
    /// into the v2 layout. The old index formula was identical
    /// (`lx + lz*32 + ly*1024` with `ly` in 0..7), so each old slot `i` maps 1:1
    /// onto the same index in the new (larger) table; old slots 8192..16384
    /// never existed and the high (y>=8) half held aliased/corrupt data that is
    /// intentionally dropped. Live chunk records are copied byte-for-byte and
    /// laid out contiguously after the new header.
    fn migrate_v1(path: PathBuf, mut old_file: File) -> IoResult<Self> {
        log::info!(
            "[Storage] Migrating old-format region file to v2 (8-height -> {}-height): {}",
            REGION_HEIGHT,
            path.display()
        );

        old_file.seek(SeekFrom::Start(0))?;
        let mut old_header = vec![0u8; OLD_V1_HEADER_SIZE];
        old_file.read_exact(&mut old_header)?;

        let mut old_locations = [0u32; OLD_V1_CHUNKS_PER_REGION];
        let mut old_timestamps = [0u32; OLD_V1_CHUNKS_PER_REGION];
        for i in 0..OLD_V1_CHUNKS_PER_REGION {
            let off = i * 4;
            old_locations[i] = u32::from_be_bytes([
                old_header[off],
                old_header[off + 1],
                old_header[off + 2],
                old_header[off + 3],
            ]);
            let ts_off = OLD_V1_CHUNKS_PER_REGION * 4 + i * 4;
            old_timestamps[i] = u32::from_be_bytes([
                old_header[ts_off],
                old_header[ts_off + 1],
                old_header[ts_off + 2],
                old_header[ts_off + 3],
            ]);
        }

        let old_file_len = old_file.metadata()?.len();

        // New image: header first, live records appended contiguously.
        let mut new_image: Vec<u8> = vec![0u8; HEADER_SIZE];
        let mut new_locations = [0u32; CHUNKS_PER_REGION];
        let mut new_timestamps = [0u32; CHUNKS_PER_REGION];
        let mut cursor_bytes: usize = HEADER_SIZE;

        for (i, &loc) in old_locations.iter().enumerate() {
            if loc == 0 {
                continue;
            }
            let offset_sectors = (loc >> 8) as u64;
            let sector_count = (loc & 0xFF) as usize;
            if offset_sectors == 0 || sector_count == 0 {
                continue;
            }

            let old_byte_offset =
                offset_sectors
                    .checked_mul(SECTOR_SIZE as u64)
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "v1 chunk offset overflow during migration",
                        )
                    })?;
            let record_len = sector_count.checked_mul(SECTOR_SIZE).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "v1 chunk sector count overflow during migration",
                )
            })?;

            if old_byte_offset + record_len as u64 > old_file_len {
                log::warn!(
                    "[Storage] Skipping out-of-range chunk slot {i} while migrating {}: record extends past EOF",
                    path.display()
                );
                continue;
            }

            old_file.seek(SeekFrom::Start(old_byte_offset))?;
            let mut record = vec![0u8; record_len];
            old_file.read_exact(&mut record)?;

            // Validate inner [data_len:u32 BE][data][padding] before copying through.
            let data_len =
                u32::from_be_bytes([record[0], record[1], record[2], record[3]]) as usize;
            if data_len + 4 > record_len {
                log::warn!(
                    "[Storage] Skipping corrupt chunk slot {i} while migrating {}: declared data_len {data_len} exceeds record size",
                    path.display()
                );
                continue;
            }

            new_image.extend_from_slice(&record);
            let new_offset_sectors = (cursor_bytes / SECTOR_SIZE) as u32;
            // STOR-M02: route through the guarded packer. `sector_count` is read
            // from the old file's location entry (`loc & 0xFF`), so by
            // construction it already fits u8 and this never errors here -- but
            // funnelling every packing site through `pack_location` keeps the
            // truncation guard in one place.
            new_locations[i] = Self::pack_location(new_offset_sectors as u64, sector_count)?;
            new_timestamps[i] = old_timestamps[i];
            cursor_bytes += record_len;
        }

        // Serialize new tables + marker into the header portion of the image.
        for i in 0..CHUNKS_PER_REGION {
            let off = i * 4;
            new_image[off..off + 4].copy_from_slice(&new_locations[i].to_be_bytes());
            let ts_off = CHUNKS_PER_REGION * 4 + i * 4;
            new_image[ts_off..ts_off + 4].copy_from_slice(&new_timestamps[i].to_be_bytes());
        }
        Self::write_marker_into(&mut new_image);

        // Atomically replace the original via a sibling temp file.
        let tmp_path: PathBuf = {
            let mut s = path.as_os_str().to_owned();
            s.push(".migrate.tmp");
            PathBuf::from(s)
        };
        {
            let mut tmp = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            tmp.write_all(&new_image)?;
            tmp.flush()?;
            tmp.sync_all()?;
        }
        std::fs::rename(&tmp_path, &path)?;

        // Reopen the migrated file and load it as v2.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        Self::load_v2(file)
    }

    #[inline]
    fn chunk_index(x: i32, y: i32, z: i32) -> IoResult<usize> {
        if !(0..REGION_HEIGHT).contains(&y) {
            let max_y = REGION_HEIGHT;
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("chunk Y {y} out of region range [0, {max_y})"),
            ));
        }
        let lx = x.rem_euclid(32) as usize;
        let ly = y as usize;
        let lz = z.rem_euclid(32) as usize;
        Ok(lx + lz * 32 + ly * 1024)
    }

    /// Reads a chunk's compressed data from the file.
    pub fn read_chunk(&mut self, x: i32, y: i32, z: i32) -> IoResult<Option<Vec<u8>>> {
        let index = Self::chunk_index(x, y, z)?;
        let loc = self.locations[index];
        if loc == 0 {
            return Ok(None);
        }

        let offset_sectors = (loc >> 8) as u64;
        let sector_count = (loc & 0xFF) as usize;

        if offset_sectors == 0 {
            return Ok(None);
        }

        self.file
            .seek(SeekFrom::Start(offset_sectors * SECTOR_SIZE as u64))?;

        // Read size (first 4 bytes of data)
        let mut size_buf = [0u8; 4];
        self.file.read_exact(&mut size_buf)?;
        let data_len = u32::from_be_bytes(size_buf) as usize;

        // Guard against sector_count == 0 underflow: `sector_count * SECTOR_SIZE - 4`
        // wraps to a huge value when sector_count is 0, bypassing the size check.
        let max_data_len = sector_count
            .checked_mul(SECTOR_SIZE)
            .and_then(|n| n.checked_sub(4))
            .unwrap_or(0);
        if data_len > max_data_len {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Chunk data exceeds sector count",
            ));
        }

        let mut data = vec![0u8; data_len];
        self.file.read_exact(&mut data)?;

        Ok(Some(data))
    }

    /// Writes a chunk's compressed data to the file.
    pub fn write_chunk(&mut self, x: i32, y: i32, z: i32, data: &[u8]) -> IoResult<()> {
        let index = Self::chunk_index(x, y, z)?;
        let loc = self.locations[index];
        let old_offset_sectors = (loc >> 8) as u64;
        let old_sector_count = (loc & 0xFF) as usize;

        let data_len = data.len();

        let required_sectors = (data_len + 4).div_ceil(SECTOR_SIZE);

        let mut offset_sectors = old_offset_sectors;

        if old_offset_sectors == 0 || required_sectors > old_sector_count {
            // Allocate at end of file (simple allocator for now)
            offset_sectors = self.file.metadata()?.len().div_ceil(SECTOR_SIZE as u64);

            if offset_sectors < HEADER_SECTORS as u64 {
                offset_sectors = HEADER_SECTORS as u64;
            } // Don't overwrite header
        }

        // STOR-M02: validate the packed location entry BEFORE writing any data
        // sectors. The previous `& 0xFF` packing silently truncated a chunk that
        // needed >255 sectors; funnelling through `pack_location` here surfaces
        // it as an error up front instead of leaving orphaned sectors on disk
        // and a corrupt location table behind.
        let new_loc = Self::pack_location(offset_sectors, required_sectors)?;

        // Write data
        self.file
            .seek(SeekFrom::Start(offset_sectors * SECTOR_SIZE as u64))?;
        self.file.write_all(&(data_len as u32).to_be_bytes())?;
        self.file.write_all(data)?;

        // Padding to sector boundary
        let written = 4 + data_len;
        let padding = required_sectors * SECTOR_SIZE - written;
        if padding > 0 {
            let zeros = vec![0u8; padding];
            self.file.write_all(&zeros)?;
        }

        // Update header
        self.locations[index] = new_loc;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;
        self.timestamps[index] = timestamp;

        // Write location to disk
        self.file.seek(SeekFrom::Start((index * 4) as u64))?;
        self.file.write_all(&new_loc.to_be_bytes())?;

        // Write timestamp to disk
        let ts_disk_offset = (CHUNKS_PER_REGION * 4) + index * 4;
        self.file.seek(SeekFrom::Start(ts_disk_offset as u64))?;
        self.file.write_all(&timestamp.to_be_bytes())?;

        // Bump the write-generation LAST, after the location entry is durable,
        // then flush. Generation acts as a publish flag: any reader that
        // observes generation G is guaranteed the location entries for all
        // writes <= G are already on disk. See STOR-003.
        //
        // STOR-M01: the durability claim above only holds if the flush is a
        // real sync barrier. The previous `File::flush` only drained userspace
        // buffers and silently violated this invariant -- a reader observing
        // generation G was NOT in fact guaranteed the data sectors were on
        // stable storage. `self.flush()` calls `sync_data` to make the promise
        // true. Full crash-safety (journaling / copy-on-write sectors) remains
        // deferred; see `flush`'s doc comment.
        self.generation = self.generation.wrapping_add(1);
        self.file.seek(SeekFrom::Start(GENERATION_OFFSET as u64))?;
        self.file.write_all(&self.generation.to_be_bytes())?;

        self.flush()?;

        Ok(())
    }

    /// Durably persists all buffered writes on this handle to stable storage.
    ///
    /// STOR-M01: the std `File::flush` this module previously used (both in
    /// `write_chunk` and at file creation) is NOT a durability barrier -- it
    /// only drains the userspace buffer into the kernel, leaving dirty page
    /// cache that a power loss can still drop. Combined with the in-place
    /// sector rewrite in `write_chunk`, that meant a crash after a "successful"
    /// save could corrupt the just-overwritten record or lose it entirely.
    ///
    /// This method calls `sync_data` (which forces file data and the metadata
    /// needed to reach it -- including size, which matters for the append
    /// allocator -- to stable storage), falling back to `sync_all` on platforms
    /// where `sync_data` is unsupported. The save path calls it after every
    /// `write_chunk` so the STOR-003 generation-counter publish flag is honest.
    ///
    /// NOT FULLY DONE / deferred remainder of STOR-M01: durability here is
    /// per-write, not transactional. `write_chunk` still rewrites the chunk's
    /// sectors in place and then updates the location table; a crash between
    /// the data write and the location-table write can still leave the new data
    /// unreachable from the header or the old data half-overwritten. Full
    /// crash-safety requires a write-ahead log or copy-on-write sector scheme,
    /// which is a larger redesign and out of scope for this remediation batch.
    /// (The parent-directory entry created on first `open` is also not synced;
    /// that would need a parent-dir fsync, also deferred.)
    pub fn flush(&mut self) -> IoResult<()> {
        match self.file.sync_data() {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::Unsupported => self.file.sync_all(),
            Err(e) => Err(e),
        }
    }

    /// Cheaply detects whether another handle has appended writes to this
    /// region file since this handle cached its location table, and re-reads
    /// the tables from disk if so. Returns `true` when a refresh occurred.
    ///
    /// The hot path is a single 4-byte read of the on-disk generation counter
    /// at `GENERATION_OFFSET`; the full table re-read only runs when that
    /// counter has advanced. A file shorter than `HEADER_SIZE` (partial write /
    /// crash) is treated as generation 0 / not stale rather than risk reading
    /// garbage and clobbering a possibly-newer in-memory cache.
    pub fn refresh_if_stale(&mut self) -> IoResult<bool> {
        let file_len = self.file.metadata()?.len() as usize;
        if file_len < HEADER_SIZE {
            return Ok(false);
        }

        self.file.seek(SeekFrom::Start(GENERATION_OFFSET as u64))?;
        let mut gen_buf = [0u8; 4];
        self.file.read_exact(&mut gen_buf)?;
        let on_disk_generation = u32::from_be_bytes(gen_buf);

        if on_disk_generation == self.generation {
            return Ok(false);
        }

        // Stale: re-read the location + timestamp tables from disk so future
        // read_chunk calls see the writer's appended sectors.
        self.file.seek(SeekFrom::Start(0))?;
        let mut header_buf = vec![0u8; CHUNKS_PER_REGION * 4 * 2];
        self.file.read_exact(&mut header_buf)?;
        for i in 0..CHUNKS_PER_REGION {
            let off = i * 4;
            self.locations[i] = u32::from_be_bytes([
                header_buf[off],
                header_buf[off + 1],
                header_buf[off + 2],
                header_buf[off + 3],
            ]);
            let ts_off = CHUNKS_PER_REGION * 4 + i * 4;
            self.timestamps[i] = u32::from_be_bytes([
                header_buf[ts_off],
                header_buf[ts_off + 1],
                header_buf[ts_off + 2],
                header_buf[ts_off + 3],
            ]);
        }
        self.generation = on_disk_generation;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::NamedTempFile;

    fn read_marker_from_disk(path: &Path) -> [u8; 4] {
        let mut f = std::fs::File::open(path).expect("open");
        f.seek(SeekFrom::Start(MARKER_OFFSET as u64))
            .expect("seek marker");
        let mut buf = [0u8; 4];
        f.read_exact(&mut buf).expect("read marker");
        buf
    }

    #[test]
    fn chunk_index_is_injective_across_full_height() {
        let mut seen: HashSet<usize> = HashSet::new();
        for x in 0..CHUNKS_PER_REGION_SIDE {
            for z in 0..CHUNKS_PER_REGION_SIDE {
                for y in 0..REGION_HEIGHT {
                    let idx = RegionFile::chunk_index(x, y, z).expect("in-range coord must be Ok");
                    assert!(
                        seen.insert(idx),
                        "index collision at (x={x}, y={y}, z={z}) -> {idx}"
                    );
                }
            }
        }
        assert_eq!(seen.len(), CHUNKS_PER_REGION);
    }

    #[test]
    fn chunk_index_rejects_out_of_range_y() {
        assert!(RegionFile::chunk_index(0, -1, 0).is_err());
        assert!(RegionFile::chunk_index(0, REGION_HEIGHT, 0).is_err());
        assert!(RegionFile::chunk_index(0, REGION_HEIGHT + 5, 0).is_err());
        // Top boundary is exclusive; REGION_HEIGHT - 1 is the last valid slot.
        assert!(RegionFile::chunk_index(0, 0, 0).is_ok());
        assert!(RegionFile::chunk_index(0, REGION_HEIGHT - 1, 0).is_ok());
        // Negative x/z are folded by rem_euclid and must remain Ok.
        assert!(RegionFile::chunk_index(-1, 0, -1).is_ok());
    }

    #[test]
    fn round_trip_at_block_y_480_survives() {
        let tmp = NamedTempFile::new().expect("tempfile");
        let path = tmp.path().to_path_buf();
        let payload: Vec<u8> = (0..123u8).collect();
        // Chunk y=15 covers block Y ~480 -- exactly the range the old bug corrupted.
        {
            let mut rf = RegionFile::open(&path).expect("open new");
            rf.write_chunk(5, 15, 7, &payload).expect("write y=15");
            assert_eq!(rf.read_chunk(5, 15, 7).unwrap().unwrap(), payload);
        }
        // Reopen and read back from disk.
        let mut rf = RegionFile::open(&path).expect("reopen");
        let got = rf.read_chunk(5, 15, 7).expect("read y=15").expect("some");
        assert_eq!(got, payload);
        assert_eq!(read_marker_from_disk(&path), REGION_MAGIC);
    }

    #[test]
    fn y0_and_y8_no_longer_collide() {
        let tmp = NamedTempFile::new().expect("tempfile");
        let path = tmp.path().to_path_buf();
        let low: Vec<u8> = vec![0xAA; 50];
        let high: Vec<u8> = vec![0xBB; 60];
        {
            let mut rf = RegionFile::open(&path).expect("open");
            rf.write_chunk(1, 0, 1, &low).expect("write y=0");
            rf.write_chunk(1, 8, 1, &high).expect("write y=8");
            // Immediate in-memory reads: distinct indices, no aliasing.
            assert_eq!(rf.read_chunk(1, 0, 1).unwrap().unwrap(), low);
            assert_eq!(rf.read_chunk(1, 8, 1).unwrap().unwrap(), high);
        }
        // Disk round-trip after reopen -- regression for STOR-001.
        let mut rf = RegionFile::open(&path).expect("reopen");
        assert_eq!(rf.read_chunk(1, 0, 1).unwrap().unwrap(), low);
        assert_eq!(rf.read_chunk(1, 8, 1).unwrap().unwrap(), high);
    }

    #[test]
    fn migrates_old_v1_file_to_v2() {
        // Construct a byte buffer mimicking an old (8-height, 64 KiB-header,
        // markerless) file with one live chunk at y in 0..7.
        let old_data: Vec<u8> = (0..77u8).collect();
        let data_len = old_data.len() as u32;
        let slot_y: i32 = 3;
        let old_index = (slot_y as usize) * 1024; // x=0, z=0, y=3
        let old_offset_sectors: u32 = (OLD_V1_HEADER_SIZE / SECTOR_SIZE) as u32;
        let sector_count: u32 = ((old_data.len() + 4) as u32).div_ceil(SECTOR_SIZE as u32);
        let old_loc = (old_offset_sectors << 8) | (sector_count & 0xFF);

        let mut buf = vec![0u8; OLD_V1_HEADER_SIZE];
        buf[old_index * 4..old_index * 4 + 4].copy_from_slice(&old_loc.to_be_bytes());
        let ts_off = OLD_V1_CHUNKS_PER_REGION * 4 + old_index * 4;
        let ts: u32 = 1_700_000_000;
        buf[ts_off..ts_off + 4].copy_from_slice(&ts.to_be_bytes());

        let mut record = vec![0u8; sector_count as usize * SECTOR_SIZE];
        record[0..4].copy_from_slice(&data_len.to_be_bytes());
        record[4..4 + old_data.len()].copy_from_slice(&old_data);
        buf.extend_from_slice(&record);

        // Sanity: the buffer has no marker (smaller than the new HEADER_SIZE).
        assert!(buf.len() < HEADER_SIZE);

        let tmp = NamedTempFile::new().expect("tempfile");
        tmp.as_file().write_all(&buf).expect("write old file");
        tmp.as_file().sync_all().expect("sync");

        let path = tmp.path().to_path_buf();
        let mut rf = RegionFile::open(&path).expect("migration should succeed");

        // (a) marker now present on disk at the reserved offset.
        assert_eq!(read_marker_from_disk(&path), REGION_MAGIC);
        // (b) the live y<8 chunk's bytes are preserved.
        let got = rf
            .read_chunk(0, slot_y, 0)
            .expect("read migrated chunk")
            .expect("chunk must be present after migration");
        assert_eq!(got, old_data);
        // (c) on-disk header size matches the new HEADER_SIZE.
        let file_len = std::fs::metadata(&path).expect("stat").len() as usize;
        assert!(
            file_len >= HEADER_SIZE,
            "migrated file_len={file_len} < HEADER_SIZE={HEADER_SIZE}"
        );
        // Pin the layout so a future constant change can't silently pass.
        assert_eq!(REGION_HEIGHT, 16);
        assert_eq!(CHUNKS_PER_REGION, 16384);
        assert_eq!(HEADER_SIZE, 33 * SECTOR_SIZE);
    }

    /// STOR-003: a separate read handle caching the location table at open
    /// must see chunks the writer appends later, after `refresh_if_stale`
    /// detects the bumped on-disk generation. Pins both the bug (stale read
    /// returns Ok(None)) and the fix (refresh -> visible) at the unit level.
    #[test]
    fn generation_refresh_detects_writer_updates() {
        // Pin the generation field offset: immediately after magic + version.
        assert_eq!(GENERATION_OFFSET, MARKER_OFFSET + REGION_MAGIC.len() + 4);
        assert_eq!(GENERATION_OFFSET, 131080);
        // HEADER_SIZE is unchanged by the additive generation field.
        assert_eq!(HEADER_SIZE, 33 * SECTOR_SIZE);

        let tmp = NamedTempFile::new().expect("tempfile");
        let path = tmp.path().to_path_buf();

        let payload_a: Vec<u8> = (0..123u8).collect();
        let payload_b: Vec<u8> = (200..250u8).collect();

        // Writer 1: write chunk A at (0,0,0). Bumps on-disk generation to 1.
        {
            let mut writer = RegionFile::open(&path).expect("open new");
            writer.write_chunk(0, 0, 0, &payload_a).expect("write A");
        }

        // Reader: a SEPARATE handle, caches generation G0 + locations with A.
        let mut reader = RegionFile::open(&path).expect("open reader");
        assert_eq!(
            reader.read_chunk(0, 0, 0).expect("read A"),
            Some(payload_a.clone())
        );

        // Writer 2: write chunk B at (1,0,0). Bumps on-disk generation to 2.
        {
            let mut writer = RegionFile::open(&path).expect("reopen writer");
            writer.write_chunk(1, 0, 0, &payload_b).expect("write B");
        }

        // WITHOUT refresh: the reader's cached location for B's slot is still 0
        // (empty), so it returns Ok(None). This is exactly the STOR-003 bug.
        assert_eq!(
            reader
                .read_chunk(1, 0, 0)
                .expect("stale read must not error"),
            None,
            "stale reader cache must not see the newly-written chunk"
        );

        // Refresh: detects generation 1 -> 2 on disk, re-reads location table.
        let refreshed = reader.refresh_if_stale().expect("refresh_if_stale");
        assert!(refreshed, "refresh must report true after a writer bump");

        // After refresh, B is visible AND A still reads correctly.
        assert_eq!(
            reader.read_chunk(1, 0, 0).expect("read B after refresh"),
            Some(payload_b.clone())
        );
        assert_eq!(
            reader
                .read_chunk(0, 0, 0)
                .expect("read A still works after refresh"),
            Some(payload_a.clone())
        );

        // A second refresh with no intervening write is a no-op.
        let refreshed_again = reader.refresh_if_stale().expect("refresh again");
        assert!(
            !refreshed_again,
            "refresh must report false when generation is unchanged"
        );
    }

    /// STOR-M02: a chunk whose compressed payload needs more than 255 sectors
    /// must be rejected with an explicit error at the write site, instead of
    /// the previous behaviour of silently truncating the sector count via
    /// `& 0xFF` and corrupting the location-table entry. The guard must fire
    /// BEFORE any data sectors are written, so a rejected chunk leaves the
    /// location table untouched (slot reads back as empty).
    #[test]
    fn write_chunk_rejects_oversized_payload_instead_of_truncating() {
        let tmp = NamedTempFile::new().expect("tempfile");
        let path = tmp.path().to_path_buf();
        let mut rf = RegionFile::open(&path).expect("open new");

        // 256 sectors required: payload just over the 8-bit sector-count limit.
        // required_sectors = ceil((data_len + 4) / SECTOR_SIZE) = 257 here.
        let oversized = vec![0u8; 256 * SECTOR_SIZE];
        let err = rf
            .write_chunk(0, 0, 0, &oversized)
            .expect_err("oversized chunk must be refused, not silently truncated");
        assert!(
            matches!(err.kind(), std::io::ErrorKind::InvalidData),
            "expected InvalidData for oversized chunk, got {:?}: {err}",
            err.kind()
        );

        // The rejected chunk must NOT have poisoned the location table: the
        // slot still reads as empty, so a future valid write can land there.
        assert_eq!(
            rf.read_chunk(0, 0, 0).expect("read must not error"),
            None,
            "location entry for rejected chunk must remain empty"
        );

        // The file is still usable for a subsequent valid write.
        let small = vec![0xAB; 32];
        rf.write_chunk(0, 0, 0, &small)
            .expect("valid write after rejection");
        assert_eq!(
            rf.read_chunk(0, 0, 0)
                .expect("read small")
                .expect("present"),
            small
        );
    }

    /// STOR-M01: `flush()` must be a real durability barrier (sync_data), not
    /// the std `File::flush` no-op. We can't simulate power loss in a unit
    /// test, but we CAN pin the contract: after `flush()` returns Ok, the
    /// on-disk generation counter reflects the most recent write (the
    /// durability-dependent publish flag from STOR-003), and a fresh handle
    /// opened against the same file observes the flushed state.
    #[test]
    fn flush_durable_sync_after_write() {
        let tmp = NamedTempFile::new().expect("tempfile");
        let path = tmp.path().to_path_buf();
        let payload: Vec<u8> = (0..200u8).collect();

        let generation_before;
        {
            let mut rf = RegionFile::open(&path).expect("open new");
            generation_before = rf.generation;
            rf.write_chunk(2, 4, 6, &payload).expect("write");
            // write_chunk already calls flush() (sync_data) internally; an
            // explicit second flush must also succeed and remain a no-op-ish
            // valid barrier (no error, no state regression).
            rf.flush().expect("explicit flush must succeed");
            // Generation advanced exactly once for the single write.
            assert_eq!(
                rf.generation,
                generation_before.wrapping_add(1),
                "flush must not corrupt the in-memory generation counter"
            );
        }

        // A fresh handle observes the flushed bytes -- the durable sync is what
        // makes the write survive the handle being dropped / a reopen.
        let mut rf = RegionFile::open(&path).expect("reopen");
        assert_eq!(
            rf.read_chunk(2, 4, 6).expect("read back").expect("present"),
            payload,
            "flushed write must be readable from a fresh handle"
        );
    }
}
