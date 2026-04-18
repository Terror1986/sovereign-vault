//! SovereignFlow Configuration Layer
//!
//! Loads and validates configuration from sovereign.toml.
//! All pipeline parameters are configurable without touching source code.
//! This enables hardware-specific tuning for different synthesis platforms.

use serde::Deserialize;
use std::path::Path;

/// Top-level configuration structure
#[derive(Debug, Deserialize, Clone)]
pub struct SovereignConfig {
    pub raptor:       RaptorConfig,
    pub reed_solomon: ReedSolomonConfig,
    pub hedges:       HedgesConfig,
    pub oligo:        OligoConfig,
    pub index:        IndexConfig,
    pub chaos:        ChaosSimConfig,
    pub throughput:   ThroughputConfig,
}

/// RaptorQ fountain code parameters
#[derive(Debug, Deserialize, Clone)]
pub struct RaptorConfig {
    /// Redundancy ratio (0.10 to 0.50)
    /// Higher = more erasure tolerance, lower code rate
    pub redundancy_ratio: f64,
}

/// Reed-Solomon erasure coding parameters  
#[derive(Debug, Deserialize, Clone)]
pub struct ReedSolomonConfig {
    /// Number of data shards per packet
    pub data_shards: usize,
    /// Number of parity shards per packet
    /// Can correct up to parity_shards erasures
    pub parity_shards: usize,
}

/// HEDGES inner codec parameters
#[derive(Debug, Deserialize, Clone)]
pub struct HedgesConfig {
    /// Beam search width for indel correction
    /// Higher = more accurate, slower
    pub beam_width: usize,
    /// Coding rate in bits per base (1.0 = maximum protection)
    pub rate: f64,
}

/// Oligo synthesis parameters
#[derive(Debug, Deserialize, Clone)]
pub struct OligoConfig {
    /// Total oligo length including primers (bases)
    pub target_length: usize,
    /// Payload bases per oligo (target_length minus primer overhead)
    pub payload_bases: usize,
    /// Minimum GC content (0.0 to 1.0)
    pub min_gc_content: f64,
    /// Maximum GC content (0.0 to 1.0)
    pub max_gc_content: f64,
    /// Maximum consecutive identical bases
    pub max_homopolymer_run: usize,
}

/// Sovereign audit index parameters
#[derive(Debug, Deserialize, Clone)]
pub struct IndexConfig {
    /// Storage backend: "memory" or "rocksdb"
    pub backend: String,
    /// Path for persistent index files
    pub index_path: String,
    /// Enable index compression
    pub compress: bool,
}

/// Chaos simulation parameters (for testing)
#[derive(Debug, Deserialize, Clone)]
pub struct ChaosSimConfig {
    /// Fraction of strands completely lost (0.0 to 1.0)
    pub strand_loss_rate: f64,
    /// Per-base substitution rate (0.0 to 1.0)
    pub substitution_rate: f64,
    /// Per-base insertion rate (0.0 to 1.0)
    pub insertion_rate: f64,
    /// Per-base deletion rate (0.0 to 1.0)
    pub deletion_rate: f64,
}

/// Throughput optimization parameters
#[derive(Debug, Deserialize, Clone)]
pub struct ThroughputConfig {
    /// Number of encoding threads (0 = all cores)
    pub threads: usize,
    /// Chunk size for large file processing (MB)
    pub chunk_size_mb: usize,
}

impl SovereignConfig {
    /// Load configuration from sovereign.toml
    /// Falls back to defaults if file not found
    pub fn load() -> Self {
        Self::load_from("sovereign.toml")
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &str) -> Self {
        if Path::new(path).exists() {
            let content = std::fs::read_to_string(path)
                .expect("Failed to read config file");
            let config: SovereignConfig = toml::from_str(&content)
                .expect("Failed to parse config file");
            config.validate();
            config
        } else {
            eprintln!("Warning: {} not found, using defaults", path);
            Self::default()
        }
    }

    /// Validate configuration values are within acceptable ranges
    pub fn validate(&self) {
        assert!(
            self.raptor.redundancy_ratio > 0.0 && self.raptor.redundancy_ratio < 1.0,
            "redundancy_ratio must be between 0.0 and 1.0"
        );
        assert!(
            self.reed_solomon.data_shards >= 2,
            "data_shards must be at least 2"
        );
        assert!(
            self.reed_solomon.parity_shards >= 1,
            "parity_shards must be at least 1"
        );
        assert!(
            self.oligo.min_gc_content < self.oligo.max_gc_content,
            "min_gc_content must be less than max_gc_content"
        );
        assert!(
            self.oligo.payload_bases <= self.oligo.target_length,
            "payload_bases cannot exceed target_length"
        );
    }

    /// Calculate the theoretical end-to-end code rate
    /// based on current configuration
    pub fn code_rate(&self) -> f64 {
        let yin_yang_rate = 2.0_f64;
        let hedges_multiplier = self.hedges.rate / 2.0;
        let rs_overhead = (self.reed_solomon.data_shards + self.reed_solomon.parity_shards) as f64
            / self.reed_solomon.data_shards as f64;
        let raptor_overhead = 1.0 + self.raptor.redundancy_ratio;

        yin_yang_rate * hedges_multiplier / rs_overhead / raptor_overhead
    }

    /// Print a summary of current configuration and calculated metrics
    pub fn print_summary(&self) {
        println!("SovereignFlow Configuration");
        println!("===========================");
        println!("RaptorQ redundancy:    {:.0}%", self.raptor.redundancy_ratio * 100.0);
        println!("Reed-Solomon:          {}+{} shards", 
            self.reed_solomon.data_shards, 
            self.reed_solomon.parity_shards);
        println!("HEDGES beam width:     {}", self.hedges.beam_width);
        println!("HEDGES rate:           {:.1} bits/base", self.hedges.rate);
        println!("Oligo target length:   {} bases", self.oligo.target_length);
        println!("Oligo payload:         {} bases", self.oligo.payload_bases);
        println!("Index backend:         {}", self.index.backend);
        println!("----------------------------");
        println!("Calculated code rate:  {:.3} bits/base", self.code_rate());
        println!("===========================");
    }
}

impl Default for SovereignConfig {
    fn default() -> Self {
        SovereignConfig {
            raptor: RaptorConfig {
                redundancy_ratio: 0.30,
            },
            reed_solomon: ReedSolomonConfig {
                data_shards: 4,
                parity_shards: 2,
            },
            hedges: HedgesConfig {
                beam_width: 64,
                rate: 1.0,
            },
            oligo: OligoConfig {
                target_length: 160,
                payload_bases: 128,
                min_gc_content: 0.40,
                max_gc_content: 0.60,
                max_homopolymer_run: 3,
            },
            index: IndexConfig {
                backend: "memory".to_string(),
                index_path: "./sovereign_index".to_string(),
                compress: true,
            },
            chaos: ChaosSimConfig {
                strand_loss_rate: 0.10,
                substitution_rate: 0.02,
                insertion_rate: 0.005,
                deletion_rate: 0.005,
            },
            throughput: ThroughputConfig {
                threads: 0,
                chunk_size_mb: 256,
            },
        }
    }
}
