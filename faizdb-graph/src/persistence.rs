//! Graph Persistence & Snapshot Serialization.
//!
//! Provides disk snapshot persistence with CRC32 integrity verification,
//! allowing the graph companion engine to operate standalone or reload across restarts.

use crate::graph::GraphStore;
use crc32fast::Hasher;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

/// Magic header for FaizDB Graph snapshot files ("FZGRPH\x01")
pub const GRAPH_SNAPSHOT_MAGIC: &[u8; 7] = b"FZGRPH\x01";

/// Save a GraphStore to disk with CRC32 verification
pub fn save_snapshot(graph: &GraphStore, path: impl AsRef<Path>) -> std::io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = path.with_extension("tmp");
    let file = File::create(&temp_path)?;
    let mut writer = BufWriter::new(file);

    let serialized = serde_json::to_vec(graph)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let mut hasher = Hasher::new();
    hasher.update(&serialized);
    let checksum = hasher.finalize();

    // Frame: [Magic 7B][Len 4B][CRC32 4B][Payload ...]
    writer.write_all(GRAPH_SNAPSHOT_MAGIC)?;
    writer.write_all(&(serialized.len() as u32).to_le_bytes())?;
    writer.write_all(&checksum.to_le_bytes())?;
    writer.write_all(&serialized)?;
    writer.flush()?;
    drop(writer);

    fs::rename(temp_path, path)?;
    Ok(())
}

/// Load a GraphStore from a snapshot file with CRC32 verification
pub fn load_snapshot(path: impl AsRef<Path>) -> std::io::Result<GraphStore> {
    let path = path.as_ref();
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut magic = [0u8; 7];
    reader.read_exact(&mut magic)?;
    if &magic != GRAPH_SNAPSHOT_MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid FaizDB Graph snapshot header",
        ));
    }

    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    let mut crc_buf = [0u8; 4];
    reader.read_exact(&mut crc_buf)?;
    let expected_crc = u32::from_le_bytes(crc_buf);

    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;

    let mut hasher = Hasher::new();
    hasher.update(&payload);
    let actual_crc = hasher.finalize();

    if actual_crc != expected_crc {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Corrupted Graph snapshot: CRC mismatch (expected {expected_crc}, got {actual_crc})"),
        ));
    }

    let graph: GraphStore = serde_json::from_slice(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(graph)
}
