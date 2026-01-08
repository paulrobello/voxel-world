use std::fs::{File, OpenOptions};
use std::io::{Read, Result as IoResult, Seek, SeekFrom, Write};
use std::path::Path;

/// A region file manages a 32x32 grid of chunks.
/// Format: 4KB Location Table | 4KB Timestamp Table | Data Sectors (4KB each)
pub struct RegionFile {
    file: File,
    /// Location table: index -> (offset_in_sectors, sector_count)
    locations: [u32; CHUNKS_PER_REGION],
    /// Timestamp table: index -> unix_timestamp
    timestamps: [u32; CHUNKS_PER_REGION],
}

pub const CHUNKS_PER_REGION_SIDE: i32 = 32;
pub const REGION_HEIGHT: i32 = 8; // Half of WORLD_CHUNKS_Y (16 chunks = 512 blocks)
pub const CHUNKS_PER_REGION: usize =
    (CHUNKS_PER_REGION_SIDE * CHUNKS_PER_REGION_SIDE * REGION_HEIGHT) as usize;
pub const SECTOR_SIZE: usize = 4096;
pub const HEADER_SECTORS: usize = (CHUNKS_PER_REGION * 4 * 2).div_ceil(SECTOR_SIZE);
pub const HEADER_SIZE: usize = HEADER_SECTORS * SECTOR_SIZE;

impl RegionFile {
    pub fn open<P: AsRef<Path>>(path: P) -> IoResult<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        let mut locations = vec![0u32; CHUNKS_PER_REGION];
        let mut timestamps = vec![0u32; CHUNKS_PER_REGION];

        let file_len = file.metadata()?.len();
        if file_len < HEADER_SIZE as u64 {
            // New file, initialize header
            file.set_len(HEADER_SIZE as u64)?;
            file.seek(SeekFrom::Start(0))?;
            let zero_header = vec![0u8; HEADER_SIZE];
            file.write_all(&zero_header)?;
        } else {
            // Read existing header
            file.seek(SeekFrom::Start(0))?;
            let mut header_buf = vec![0u8; HEADER_SIZE];
            file.read_exact(&mut header_buf)?;

            for i in 0..CHUNKS_PER_REGION {
                let offset = i * 4;
                locations[i] = u32::from_be_bytes([
                    header_buf[offset],
                    header_buf[offset + 1],
                    header_buf[offset + 2],
                    header_buf[offset + 3],
                ]);

                let ts_offset = (CHUNKS_PER_REGION * 4) + i * 4;
                timestamps[i] = u32::from_be_bytes([
                    header_buf[ts_offset],
                    header_buf[ts_offset + 1],
                    header_buf[ts_offset + 2],
                    header_buf[ts_offset + 3],
                ]);
            }
        }

        let mut locations_arr = [0u32; CHUNKS_PER_REGION];
        locations_arr.copy_from_slice(&locations);
        let mut timestamps_arr = [0u32; CHUNKS_PER_REGION];
        timestamps_arr.copy_from_slice(&timestamps);

        Ok(Self {
            file,
            locations: locations_arr,
            timestamps: timestamps_arr,
        })
    }

    #[inline]
    fn chunk_index(x: i32, y: i32, z: i32) -> usize {
        let lx = x.rem_euclid(32) as usize;
        let ly = y.clamp(0, REGION_HEIGHT - 1) as usize;
        let lz = z.rem_euclid(32) as usize;
        lx + lz * 32 + ly * 1024
    }

    /// Reads a chunk's compressed data from the file.
    pub fn read_chunk(&mut self, x: i32, y: i32, z: i32) -> IoResult<Option<Vec<u8>>> {
        let index = Self::chunk_index(x, y, z);
        let loc = self.locations[index];
        if loc == 0 {
            return Ok(None);
        }

        let offset_sectors = (loc >> 8) as u64;
        let sector_count = (loc & 0xFF) as usize;

        if offset_sectors == 0 {
            return Ok(None);
        }

        println!(
            "[Region] Reading chunk ({}, {}, {}) from sector {} (count {})",
            x, y, z, offset_sectors, sector_count
        );

        self.file
            .seek(SeekFrom::Start(offset_sectors * SECTOR_SIZE as u64))?;

        // Read size (first 4 bytes of data)
        let mut size_buf = [0u8; 4];
        self.file.read_exact(&mut size_buf)?;
        let data_len = u32::from_be_bytes(size_buf) as usize;

        if data_len > sector_count * SECTOR_SIZE - 4 {
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
        let index = Self::chunk_index(x, y, z);
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
        let new_loc = ((offset_sectors as u32) << 8) | (required_sectors as u32 & 0xFF);
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

        self.file.flush()?;

        Ok(())
    }
}
