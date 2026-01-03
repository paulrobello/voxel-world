# Refactor & Logic Review (2026-01-03)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

# Phase 3: World Persistence - Comprehensive Plan

## Objectives
- **Scalability**: Support infinite worlds via region-based storage.
- **Performance**: Zero frame-drop saving/loading via asynchronous I/O and threading.
- **Network-Ready**: Data structures designed for direct network streaming (Phase 7).
- **Extensibility**: Versioned formats to support future features (Entities, AI).

## 1. File Format Specification

### 1.1 Region Files (`r.x.z.vxr`)
We will adopt a region-based approach (similar to Anvil) to minimize file handle overhead and filesystem fragmentation. This format allows fast random access to chunks.

- **Region Size**: 32x32 chunks (1024 chunks per file).
- **Naming**: `r.{region_x}.{region_z}.vxr`, where `region_x = chunk_x >> 5` (floor division).
- **Location**: stored in `world_name/region/` directory.

### 1.2 Header Structure (8KB)
The first 8KB of the file is reserved for headers to allow O(1) lookups.
- **Location Table (0x0000 - 0x0FFF)**: 1024 entries of 4 bytes.
    - `offset` (3 bytes): Offset in 4KB sectors from start of file.
    - `sector_count` (1 byte): Number of 4KB sectors occupied by this chunk.
    - Formula: `index = (chunk_x & 31) + (chunk_z & 31) * 32`.
- **Timestamp Table (0x1000 - 0x1FFF)**: 1024 entries of 4 bytes (u32 unix timestamp).
    - Tracks when the chunk was last modified. useful for syncing.

### 1.3 Chunk Data Payload
Each chunk is stored in a compressed blob starting at the sector offset.
- **Compression**: **Zstd**. It offers superior decompression speed compared to Gzip/Deflate, which is critical for minimizing latency during network streaming and gameplay.
- **Serialization**: Binary format (e.g., `bincode`).
- **Padding**: Payload is padded to the nearest 4KB sector boundary.

### 1.4 Data Schema (Version 1)
The serialized payload will mirror the runtime structure but optimized for storage/network.

```rust
// Pseudo-struct for serialization
struct SerializedChunk {
    version: u8,                // Format version (1)
    flags: u8,                  // Bitmask (is_generated, has_entities, etc.)
    block_data: Vec<u8>,        // 32^3 bytes (compressed) or Palette+Indices
    metadata: Vec<BlockMeta>,   // Sparse map: index -> (model_id, rotation)
    // Future expansion (Phase 6)
    // entities: Vec<EntitySnapshot>,
    // tile_entities: Vec<TileEntity>,
}

struct BlockMeta {
    index: u16, // flattened index in chunk
    data: u16,  // packed model_id (8) + rotation (2) + extra (6)
}
```

## 2. Systems Architecture

### 2.1 Storage Module (`src/storage/`)
A new module to handle all persistence logic.
- **`RegionManager`**: Manages open file handles. Uses an LRU cache (e.g., max 16 open region files) to prevent OS resource exhaustion.
- **`AsyncIoWorker`**: A background thread (using `std::thread` and channels or `tokio` if we pull that in, but simple threads preferred for now) that handles the actual compression/writing and reading/decompression.
    - **Read Queue**: High priority (player movement).
    - **Write Queue**: Low priority (auto-save).

### 2.2 Integration with World
- **`ChunkLoader`**: Modified to check `Storage` before generating terrain.
    - Flow: `Request Chunk` -> `Check Disk` -> `(Found) Load & Decompress` -> `Return`.
    - Fallback: `(Not Found)` -> `Generate Terrain`.
- **`AutoSaveSystem`**:
    - Runs every N seconds (e.g., 30s).
    - Scans `World` for chunks with `is_dirty = true`.
    - Clones chunk data (snapshot) to send to `AsyncIoWorker` to avoid locking the render thread.

### 2.3 Network Alignment (Future Proofing)
The serialization format is designed to be the **exact payload** sent over the network.
- **Streaming**: When a client requests a chunk, the server reads the compressed blob from the region file and sends it directly over the socket. No deserialization/reserialization on the server.
- **Latency**: Zstd decompression is extremely fast, minimizing client-side hitching.

## 3. Detailed Implementation Plan

### Step 1: Core Data Structures
- [x] Create `src/storage/mod.rs` and `src/storage/format.rs`.
- [x] Define the `SerializedChunk` struct and helper methods.
- [x] Implement conversion traits: `TryFrom<&Chunk> for SerializedChunk` and `TryInto<Chunk> for SerializedChunk`.
- [x] **Test**: Write a unit test that creates a chunk with random blocks/models, serializes it, deserializes it, and asserts equality.

### Step 2: Region File Logic
- [x] Create `src/storage/region.rs`.
- [x] Implement `RegionFile` struct.
    - `open(path)`: Opens or creates file, parses header.
    - `read_chunk(x, z)`: Looks up offset, reads data, returns `Vec<u8>`.
    - `write_chunk(x, z, data)`: Finds free sector or appends to end, updates header.
    - **Fragmentation Handling**: If new data > old sector count, mark old sectors free (bitmap) and allocate new ones at end of file. (Simple allocator).

### Step 3: Async I/O Layer
- [x] Create `src/storage/worker.rs`.
- [x] exact `std::sync::mpsc` channels for communicating between Main Thread and I/O Thread.
- [x] Implement the command loop: `Load(pos, reply_channel)`, `Save(pos, data)`.

### Step 4: World Integration
- [x] Modify `World` struct to hold the `Storage` system.
- [x] Update `ChunkLoader` to query storage.
- [x] Implement `is_dirty` tracking in `BlockUpdate` and `BlockModification` logic.
- [x] Add `world/level.dat` for seed and global metadata.

### Step 5: Safety & Migration
- [x] **Backups**: On world load, if version < current, backup `level.dat`.
- [x] **Error Handling**: Corrupt chunk recovery (log error, regenerate chunk). Do not crash.

## 4. Expansion Hooks

### 4.1 Entities
The `SerializedChunk` struct will have an `entities` field. When Phase 6 arrives, we just populate this vector. The region file format doesn't change.

### 4.2 Multiplayer
The `block_data` in `SerializedChunk` corresponds to the bulk chunk data packet.
- **Delta Compression**: For live updates, we will send individual `BlockChange` packets. The full chunk is only sent on load.

### 4.3 Custom Models
Sub-voxel models are part of the `metadata` field. If the model registry grows, the `model_id` might need to expand from u8 to u16. The `BlockMeta` struct should reserve bits or use a variable-length scheme if strict packing is needed, but currently `u16` data (8 id, 2 rot, 6 reserved) is fine for 256 models.

## 5. Low Latency Considerations
- **Budgeting**: The `AutoSaveSystem` should limit how many chunks it snapshots per frame to prevent RAM bandwidth spikes.
- **Zero-Copy**: Use `bytemuck` for casting byte slices to header arrays where safe.
- **Priority**: Load requests always preempt Save requests in the worker queue.
