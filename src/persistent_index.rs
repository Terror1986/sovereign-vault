//! Persistent Sovereign Index -- RocksDB Backend
//!
//! Replaces the in-memory HashMap with a persistent key-value store
//! that scales to 1TB+ of strand data without memory constraints.
//!
//! Key structure: "packet_index:shard_index" -> BLAKE3 hash (8 hex chars)
//! Example: "000042:003" -> "a1b2c3d4"
//!
//! At 1TB scale with 64-byte shards:
//!   ~16 billion packets * 6 shards = ~96 billion entries
//!   Each entry: ~20 bytes key + 8 bytes value = ~28 bytes
//!   Total index size: ~2.7 GB -- easily fits on any storage system

use rocksdb::{DB, Options, WriteOptions};
use std::path::Path;

pub struct PersistentIndex {
    db: DB,
    path: String,
}

impl PersistentIndex {
    /// Open or create a persistent index at the given path
    pub fn open(path: &str) -> Result<Self, String> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        // Optimize for write-heavy workload during encoding
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB write buffer
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base(64 * 1024 * 1024);
        // Bloom filter reduces disk reads during audit
        opts.set_bloom_locality(1);

        let db = DB::open(&opts, path)
            .map_err(|e| format!("Failed to open index at {}: {}", path, e))?;

        Ok(PersistentIndex {
            db,
            path: path.to_string(),
        })
    }

    /// Insert a strand hash into the index
    pub fn insert(&self, packet_index: usize, shard_index: usize, hash: &str) -> Result<(), String> {
        let key = Self::make_key(packet_index, shard_index);
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(false); // async writes for throughput
        self.db.put_opt(key.as_bytes(), hash.as_bytes(), &write_opts)
            .map_err(|e| format!("Insert failed: {}", e))
    }

    /// Look up a strand hash from the index
    pub fn get(&self, packet_index: usize, shard_index: usize) -> Option<String> {
        let key = Self::make_key(packet_index, shard_index);
        self.db.get(key.as_bytes())
            .ok()
            .flatten()
            .map(|v| String::from_utf8(v).unwrap_or_default())
    }

    /// Verify a strand hash matches what's in the index
    pub fn verify(&self, packet_index: usize, shard_index: usize, hash: &str) -> bool {
        self.get(packet_index, shard_index)
            .map(|stored| stored == hash)
            .unwrap_or(false)
    }

    /// Flush all pending writes to disk
    pub fn flush(&self) -> Result<(), String> {
        self.db.flush()
            .map_err(|e| format!("Flush failed: {}", e))
    }

    /// Get index statistics
    pub fn stats(&self) -> IndexStats {
        let estimated_keys = self.db
            .property_int_value("rocksdb.estimate-num-keys")
            .ok()
            .flatten()
            .unwrap_or(0);
        
        let size_bytes = self.db
            .property_int_value("rocksdb.total-sst-files-size")
            .ok()
            .flatten()
            .unwrap_or(0);

        IndexStats {
            estimated_entries: estimated_keys,
            size_bytes,
            path: self.path.clone(),
        }
    }

    /// Delete the index (for cleanup after successful recovery)
    pub fn destroy(path: &str) -> Result<(), String> {
        DB::destroy(&Options::default(), path)
            .map_err(|e| format!("Destroy failed: {}", e))
    }

    fn make_key(packet_index: usize, shard_index: usize) -> String {
        format!("{:08}:{:03}", packet_index, shard_index)
    }
}

#[derive(Debug)]
pub struct IndexStats {
    pub estimated_entries: u64,
    pub size_bytes: u64,
    pub path: String,
}

impl std::fmt::Display for IndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Entries: ~{} | Size: {:.2} MB | Path: {}",
            self.estimated_entries,
            self.size_bytes as f64 / 1_048_576.0,
            self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persistent_index_roundtrip() {
        let path = "/tmp/sovereign_test_index";
        
        // Clean up any previous test
        let _ = PersistentIndex::destroy(path);
        
        // Create index
        let index = PersistentIndex::open(path).unwrap();
        
        // Insert some entries
        index.insert(0, 0, "a1b2c3d4").unwrap();
        index.insert(0, 1, "e5f6g7h8").unwrap();
        index.insert(1000000, 5, "deadbeef").unwrap();
        
        // Verify retrieval
        assert_eq!(index.get(0, 0), Some("a1b2c3d4".to_string()));
        assert_eq!(index.get(0, 1), Some("e5f6g7h8".to_string()));
        assert_eq!(index.get(1000000, 5), Some("deadbeef".to_string()));
        assert_eq!(index.get(99, 99), None);
        
        // Verify hash checking
        assert!(index.verify(0, 0, "a1b2c3d4"));
        assert!(!index.verify(0, 0, "wronghash"));
        
        // Print stats
        index.flush().unwrap();
        println!("Index stats: {}", index.stats());
        
        // Cleanup
        drop(index);
        PersistentIndex::destroy(path).unwrap();
        
        println!("Persistent index roundtrip: PASS ✅");
    }
}
