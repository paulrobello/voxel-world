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
                        eprintln!("[Storage] Failed to save chunk at {:?}: {}", pos, e);
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
                // println!("[Storage] Loaded chunk at {:?}", pos);
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

        // println!("[Storage] Saving chunk at {:?}", pos);
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

    pub fn load_chunk(&self, pos: ChunkPos) -> Result<Option<Chunk>, String> {
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

    pub fn save_chunk(&self, pos: ChunkPos, chunk: SerializedChunk) {
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
