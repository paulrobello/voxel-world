use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use super::region::{CHUNKS_PER_REGION_SIDE, RegionFile};
use super::{Chunk, SerializedChunk, compress_chunk, decompress_chunk};
use crate::world::ChunkPos;

pub enum StorageCommand {
    Load {
        pos: ChunkPos,
        reply: Sender<Result<Option<Chunk>, String>>,
    },
    Save {
        pos: ChunkPos,
        chunk: SerializedChunk,
    },
    Shutdown,
}

pub struct StorageWorker {
    world_dir: PathBuf,
    regions: HashMap<(i32, i32), RegionFile>,
}

impl StorageWorker {
    pub fn new(world_dir: PathBuf) -> Self {
        Self {
            world_dir,
            regions: HashMap::new(),
        }
    }

    pub fn run(mut self, receiver: Receiver<StorageCommand>) {
        while let Ok(cmd) = receiver.recv() {
            match cmd {
                StorageCommand::Load { pos, reply } => {
                    let result = self.load_chunk(pos);
                    let _ = reply.send(result);
                }
                StorageCommand::Save { pos, chunk } => {
                    if let Err(e) = self.save_chunk(pos, chunk) {
                        log::warn!("[Storage] Failed to save chunk at {:?}: {}", pos, e);
                    }
                }
                StorageCommand::Shutdown => break,
            }
        }
    }

    fn get_region(&mut self, rx: i32, rz: i32) -> Result<&mut RegionFile, String> {
        if !self.regions.contains_key(&(rx, rz)) {
            let region_dir = self.world_dir.join("region");
            if !region_dir.exists() {
                std::fs::create_dir_all(&region_dir).map_err(|e: std::io::Error| e.to_string())?;
            }
            let path = region_dir.join(format!("r.{}.{}.vxr", rx, rz));
            let region = RegionFile::open(path).map_err(|e: std::io::Error| e.to_string())?;
            self.regions.insert((rx, rz), region);
        }
        Ok(self.regions.get_mut(&(rx, rz)).unwrap())
    }

    fn load_chunk(&mut self, pos: ChunkPos) -> Result<Option<Chunk>, String> {
        let rx = pos.x.div_euclid(CHUNKS_PER_REGION_SIDE);
        let rz = pos.z.div_euclid(CHUNKS_PER_REGION_SIDE);

        let region = self.get_region(rx, rz)?;
        match region
            .read_chunk(pos.x, pos.y, pos.z)
            .map_err(|e: std::io::Error| e.to_string())?
        {
            Some(data) => {
                // log::debug!("[Storage] Loaded chunk at {:?}", pos);
                let serialized = decompress_chunk(&data)?;
                let chunk = Chunk::try_from(serialized)?;
                Ok(Some(chunk))
            }
            None => Ok(None),
        }
    }

    fn save_chunk(&mut self, pos: ChunkPos, chunk: SerializedChunk) -> Result<(), String> {
        let rx = pos.x.div_euclid(CHUNKS_PER_REGION_SIDE);
        let rz = pos.z.div_euclid(CHUNKS_PER_REGION_SIDE);

        // log::debug!("[Storage] Saving chunk at {:?}", pos);
        let data = compress_chunk(&chunk)?;
        let region = self.get_region(rx, rz)?;
        region
            .write_chunk(pos.x, pos.y, pos.z, &data)
            .map_err(|e: std::io::Error| e.to_string())?;
        Ok(())
    }
}

pub struct StorageSystem {
    tx: Sender<StorageCommand>,
    worker_thread: Option<thread::JoinHandle<()>>,
}

impl StorageSystem {
    pub fn new(world_dir: PathBuf) -> Self {
        let (tx, rx) = channel();
        let worker = StorageWorker::new(world_dir);
        let worker_thread = thread::spawn(move || {
            worker.run(rx);
        });

        Self {
            tx,
            worker_thread: Some(worker_thread),
        }
    }

    pub fn load_chunk<S: Into<ChunkPos>>(&self, pos: S) -> Result<Option<Chunk>, String> {
        let pos = pos.into();
        let (reply_tx, reply_rx) = channel();
        self.tx
            .send(StorageCommand::Load {
                pos,
                reply: reply_tx,
            })
            .map_err(|e| e.to_string())?;
        reply_rx
            .recv()
            .map_err(|e: std::sync::mpsc::RecvError| e.to_string())?
    }

    pub fn save_chunk<S: Into<ChunkPos>>(&self, pos: S, chunk: SerializedChunk) {
        let pos = pos.into();
        let _ = self.tx.send(StorageCommand::Save { pos, chunk });
    }
}

impl Drop for StorageSystem {
    fn drop(&mut self) {
        let _ = self.tx.send(StorageCommand::Shutdown);
        if let Some(handle) = self.worker_thread.take() {
            let _ = handle.join();
        }
    }
}

/// A lightweight, non-blocking storage reader for parallel chunk loading.
///
/// Unlike `StorageSystem` which uses a single worker thread for all I/O,
/// `ParallelStorageReader` allows each chunk loader worker to read directly
/// from disk with its own region file cache. This enables true parallel I/O.
///
/// Use this for reads in chunk loader workers. Use `StorageSystem` for writes
/// (which are async and don't block).
pub struct ParallelStorageReader {
    world_dir: PathBuf,
    regions: HashMap<(i32, i32), RegionFile>,
}

impl ParallelStorageReader {
    pub fn new(world_dir: PathBuf) -> Self {
        Self {
            world_dir,
            regions: HashMap::new(),
        }
    }

    fn get_region(&mut self, rx: i32, rz: i32) -> Result<&mut RegionFile, String> {
        if !self.regions.contains_key(&(rx, rz)) {
            let region_dir = self.world_dir.join("region");
            if !region_dir.exists() {
                // Don't create directories - this is read-only
                // If the region directory doesn't exist, no chunks have been saved
                return Err("Region directory does not exist".to_string());
            }
            let path = region_dir.join(format!("r.{}.{}.vxr", rx, rz));
            if !path.exists() {
                // Region file doesn't exist - no chunks saved in this region
                return Err("Region file does not exist".to_string());
            }
            let region = RegionFile::open(path).map_err(|e: std::io::Error| e.to_string())?;
            self.regions.insert((rx, rz), region);
        }
        Ok(self.regions.get_mut(&(rx, rz)).unwrap())
    }

    /// Loads a chunk from disk without blocking other workers.
    ///
    /// Returns `Ok(None)` if the chunk doesn't exist on disk.
    /// Returns `Ok(Some(chunk))` if the chunk was loaded successfully.
    /// Returns `Err` only for actual I/O errors (not "file doesn't exist").
    pub fn load_chunk<S: Into<ChunkPos>>(&mut self, pos: S) -> Result<Option<Chunk>, String> {
        let pos = pos.into();
        let rx = pos.x.div_euclid(CHUNKS_PER_REGION_SIDE);
        let rz = pos.z.div_euclid(CHUNKS_PER_REGION_SIDE);

        // Try to get region - if it doesn't exist, chunk isn't saved
        let region = match self.get_region(rx, rz) {
            Ok(r) => r,
            Err(_) => return Ok(None), // Region doesn't exist = chunk not saved
        };

        // STOR-003: this handle's cached location table may be stale if the
        // writer appended a chunk to this region since we first opened it.
        // Probe the on-disk generation and re-read the tables if it advanced.
        // A refresh I/O error is propagated like a read error, never swallowed
        // into Ok(None) (which would silently lose the player's edit).
        region
            .refresh_if_stale()
            .map_err(|e: std::io::Error| e.to_string())?;

        match region
            .read_chunk(pos.x, pos.y, pos.z)
            .map_err(|e: std::io::Error| e.to_string())?
        {
            Some(data) => {
                let serialized = decompress_chunk(&data)?;
                let chunk = Chunk::try_from(serialized)?;
                Ok(Some(chunk))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{BlockType, Chunk};
    use tempfile::tempdir;

    /// STOR-003 regression at the worker level: a `ParallelStorageReader`
    /// that cached a region before the writer appended a chunk to it must see
    /// the freshly-saved chunk on its next read (via the region's
    /// `refresh_if_stale`), not return `Ok(None)` and trigger a seed-regen
    /// that loses the player's edit.
    ///
    /// Determinism: the `StorageSystem` worker drains its mpsc channel in send
    /// order, and `save_chunk` flushes on every write. So a `load_chunk` that
    /// round-trips through the same worker after a `save_chunk` only returns
    /// once the save has been processed and flushed to disk -- a sync point the
    /// reader relies on before opening / re-reading the file.
    #[test]
    fn parallel_reader_sees_writer_save_after_refresh() {
        let dir = tempdir().expect("tempdir");
        let writer = StorageSystem::new(dir.path().to_path_buf());

        let pos_a = ChunkPos::new(0, 0, 0);
        let pos_b = ChunkPos::new(1, 0, 0); // same region (0,0) as pos_a

        // Chunk A -- written first so the reader can open and cache the region.
        let mut chunk_a = Chunk::new();
        chunk_a.set_block(0, 0, 0, BlockType::Stone);
        writer.save_chunk(pos_a, SerializedChunk::from(&chunk_a));
        // Sync point: guarantees the Save is durable on disk before the reader
        // touches the file (channel FIFO; worker handles Save then Load).
        assert!(
            writer.load_chunk(pos_a).unwrap().is_some(),
            "writer must see its own save of chunk A"
        );

        // Reader opens the region now: caches generation G0 + locations with A.
        let mut reader = ParallelStorageReader::new(dir.path().to_path_buf());
        assert!(
            reader.load_chunk(pos_a).unwrap().is_some(),
            "reader must see chunk A written before it opened the region"
        );

        // Writer saves chunk B in the same region: bumps on-disk generation.
        let mut chunk_b = Chunk::new();
        chunk_b.set_block(2, 2, 2, BlockType::Dirt);
        writer.save_chunk(pos_b, SerializedChunk::from(&chunk_b));
        // Sync point: ensures B is durable on disk before the reader reads.
        assert!(
            writer.load_chunk(pos_b).unwrap().is_some(),
            "writer must see chunk B"
        );

        // Reader must see B -- refresh_if_stale detects the bumped generation
        // and re-reads the location table. Without the refresh this returns
        // Ok(None) and the save is silently lost (the STOR-003 bug).
        let loaded_b = reader
            .load_chunk(pos_b)
            .expect("reader load must not error")
            .expect("reader must see freshly-saved chunk after refresh");
        assert_eq!(
            loaded_b.get_block(2, 2, 2),
            BlockType::Dirt,
            "refreshed reader must return the edited block data"
        );

        // A is still readable through the same refreshed cache.
        assert!(
            reader.load_chunk(pos_a).unwrap().is_some(),
            "chunk A must still be readable after the refresh"
        );

        // `writer` (and thus its worker thread) drops before `dir`, so the
        // tempdir cleanup never races an in-flight save.
    }
}
