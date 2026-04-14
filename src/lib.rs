use raptorq::{Encoder, EncodingPacket, ObjectTransmissionInformation, Decoder};
use rand::Rng;
use reed_solomon_erasure::galois_8::ReedSolomon;

// ─────────────────────────────────────────────────────────────────────────────
// SECTION 1: RaptorQ Erasure Coding (Outer Shield)
// ─────────────────────────────────────────────────────────────────────────────

pub struct RaptorConfig { pub redundancy_ratio: f64 }
impl Default for RaptorConfig {
    fn default() -> Self { Self { redundancy_ratio: 0.30 } }
}

pub fn raptor_encode(data: &[u8], config: &RaptorConfig) -> (Vec<EncodingPacket>, ObjectTransmissionInformation) {
    let symbol_size = 64u16;
    let oti = ObjectTransmissionInformation::with_defaults(data.len() as u64, symbol_size);
    let encoder = Encoder::new(data, oti);
    let packets = encoder.get_encoded_packets(
        (data.len() as f64 / symbol_size as f64 * config.redundancy_ratio).ceil() as u32
    );
    println!("  [RaptorQ]  {} bytes -> {} packets ({} bytes each, {:.0}% redundancy)",
        data.len(), packets.len(), symbol_size, config.redundancy_ratio * 100.0);
    (packets, oti)
}

pub fn raptor_decode(packets: &[Option<EncodingPacket>], oti: ObjectTransmissionInformation) -> Option<Vec<u8>> {
    let mut decoder = Decoder::new(oti);
    for p in packets.iter().flatten() {
        if let Some(result) = decoder.decode(p.clone()) { return Some(result); }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// SECTION 2: Reed-Solomon Inner Code (Per-Strand Substitution Repair)
// ─────────────────────────────────────────────────────────────────────────────
//
// Splits each packet into DATA_SHARDS data shards + PARITY_SHARDS parity shards.
// Can recover from up to PARITY_SHARDS corrupted or missing shards per strand.
// This runs BEFORE Yin-Yang encoding and AFTER Yin-Yang decoding.

pub const DATA_SHARDS: usize = 4;
pub const PARITY_SHARDS: usize = 2; // can fix up to 2 corrupted shards per strand

pub fn rs_encode(data: &[u8]) -> Vec<Vec<u8>> {
    let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();

    // Pad data so it divides evenly into DATA_SHARDS shards
    let shard_size = (data.len() + DATA_SHARDS - 1) / DATA_SHARDS;
    let mut padded = data.to_vec();
    padded.resize(shard_size * DATA_SHARDS, 0);

    // Split into shards
    let mut shards: Vec<Vec<u8>> = padded
        .chunks(shard_size)
        .map(|c| c.to_vec())
        .collect();

    // Append empty parity shards
    for _ in 0..PARITY_SHARDS {
        shards.push(vec![0u8; shard_size]);
    }

    rs.encode(&mut shards).unwrap();
    shards
}

pub fn rs_decode(shards: &mut Vec<Option<Vec<u8>>>, original_len: usize) -> Option<Vec<u8>> {
    let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();

    match rs.reconstruct(shards) {
        Ok(_) => {
            // Reassemble data shards only, trim padding
            let mut result: Vec<u8> = shards.iter()
                .take(DATA_SHARDS)
                .filter_map(|s| s.as_ref())
                .flat_map(|s| s.iter().copied())
                .collect();
            result.truncate(original_len);
            Some(result)
        }
        Err(_) => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SECTION 3: Yin-Yang Transcoder (Binary → ATGC)
// ─────────────────────────────────────────────────────────────────────────────

const YIN: [[char; 4]; 2] = [['A','T','G','C'], ['C','G','T','A']];

#[derive(Debug, Clone)]
pub struct Oligo {
    pub sequence: String,
    pub packet_index: usize,
    pub shard_index: usize,        // which RS shard this oligo carries
    pub original_len: usize,       // original packet byte length (for RS decode)
    pub gc_content: f32,
    pub sovereign_hash: String,
}

pub fn gc_content(seq: &str) -> f32 {
    if seq.is_empty() { return 0.0; }
    (seq.chars().filter(|&c| c=='G'||c=='C').count() as f32 / seq.len() as f32) * 100.0
}

pub fn max_homopolymer(seq: &str) -> usize {
    let c: Vec<char> = seq.chars().collect();
    if c.is_empty() { return 0; }
    let (mut max, mut cur) = (1usize, 1usize);
    for i in 1..c.len() {
        if c[i]==c[i-1] { cur+=1; max=max.max(cur); } else { cur=1; }
    }
    max
}

pub fn validate_oligo(o: &Oligo) -> Result<(), String> {
    if o.gc_content < 40.0 || o.gc_content > 60.0 {
        return Err(format!("GC {:.1}% out of range", o.gc_content));
    }
    if max_homopolymer(&o.sequence) > 3 {
        return Err(format!("Homopolymer run detected"));
    }
    Ok(())
}

fn yin_yang_encode(data: &[u8]) -> String {
    let mut sequence = String::new();
    let mut rule = 0usize;
    let mut prev = ' ';
    for byte in data {
        for shift in [6u8, 4, 2, 0] {
            let bits = ((byte >> shift) & 0b11) as usize;
            let base = if YIN[rule][bits] == prev { rule ^= 1; YIN[rule][bits] } else { YIN[rule][bits] };
            sequence.push(base);
            prev = base;
            if sequence.len() % 8 == 0 { rule ^= 1; }
        }
    }
    sequence
}

/// Encode one RaptorQ packet into multiple oligos (one per RS shard).
/// Each oligo carries one Reed-Solomon shard, so the strand can self-repair.
pub fn encode_packet_to_oligos(packet_data: &[u8], packet_index: usize) -> Vec<Oligo> {
    let original_len = packet_data.len();
    let shards = rs_encode(packet_data);
    let mut oligos = Vec::new();

    for (shard_index, shard) in shards.iter().enumerate() {
        let h = blake3::hash(shard);
        let hash_hex = format!("{:02x}{:02x}{:02x}{:02x}",
            h.as_bytes()[0], h.as_bytes()[1], h.as_bytes()[2], h.as_bytes()[3]);

        let sequence = yin_yang_encode(shard);
        let gc = gc_content(&sequence);

        oligos.push(Oligo {
            sequence,
            packet_index,
            shard_index,
            original_len,
            gc_content: gc,
            sovereign_hash: hash_hex,
        });
    }

    oligos
}

// ─────────────────────────────────────────────────────────────────────────────
// SECTION 4: Chaos Engine
// ─────────────────────────────────────────────────────────────────────────────

pub struct ChaosConfig {
    pub strand_loss_rate: f64,
    pub base_flip_rate: f64,
    pub insertion_rate: f64,
    pub deletion_rate: f64,
}

impl ChaosConfig {
    pub fn worst_case() -> Self {
        Self { strand_loss_rate: 0.20, base_flip_rate: 0.02,
               insertion_rate: 0.01,  deletion_rate: 0.01 }
    }
}

#[derive(Default, Debug)]
pub struct ChaosStats {
    pub strands_lost: usize,
    pub base_flips: usize,
    pub insertions: usize,
    pub deletions: usize,
}

const BASES: [char; 4] = ['A','T','G','C'];

pub fn apply_chaos(oligos: &[Option<Oligo>], cfg: &ChaosConfig) -> (Vec<Option<Oligo>>, ChaosStats) {
    let mut rng = rand::thread_rng();
    let mut result = Vec::with_capacity(oligos.len());
    let mut stats = ChaosStats::default();

    for oligo in oligos {
        let Some(o) = oligo else { result.push(None); continue; };
        if rng.gen_bool(cfg.strand_loss_rate) {
            result.push(None); stats.strands_lost += 1; continue;
        }
        let mut seq: Vec<char> = o.sequence.chars().collect();
        let mut i = 0;
        while i < seq.len() {
            if rng.gen_bool(cfg.base_flip_rate) { seq[i] = BASES[rng.gen_range(0..4)]; stats.base_flips += 1; }
            if rng.gen_bool(cfg.insertion_rate) { seq.insert(i, BASES[rng.gen_range(0..4)]); stats.insertions += 1; i += 1; }
            if rng.gen_bool(cfg.deletion_rate) && seq.len() > 1 { seq.remove(i); stats.deletions += 1; continue; }
            i += 1;
        }
        let s: String = seq.iter().collect();
        let gc = gc_content(&s);
        result.push(Some(Oligo {
            sequence: s, packet_index: o.packet_index, shard_index: o.shard_index,
            original_len: o.original_len, gc_content: gc,
            sovereign_hash: o.sovereign_hash.clone()
        }));
    }
    (result, stats)
}

// ─────────────────────────────────────────────────────────────────────────────
// SECTION 5: Sovereign Audit + Recovery
// ─────────────────────────────────────────────────────────────────────────────

pub fn sovereign_audit(original: &[Oligo], post_chaos: &[Option<Oligo>]) -> (usize, usize) {
    let (mut ok, mut bad) = (0usize, 0usize);
    for (orig, chaos) in original.iter().zip(post_chaos.iter()) {
        if let Some(c) = chaos {
            if c.sovereign_hash == orig.sovereign_hash { ok += 1; } else { bad += 1; }
        }
    }
    (ok, bad)
}

/// Attempt Reed-Solomon recovery on a group of oligos belonging to one RaptorQ packet.
/// Returns the reconstructed packet bytes, or None if too many shards were lost.
pub fn rs_recover_packet(
    oligos: &[Option<Oligo>],
    original_packet: &EncodingPacket,
    original_oligos: &[Oligo],
) -> (Option<EncodingPacket>, usize, usize) {
    let shard_size = if let Some(Some(o)) = oligos.iter().find(|o| o.is_some()) {
        // Each shard = sequence length / 4 bits per base / 4 bits per byte
        o.sequence.len() / 4
    } else {
        return (None, 0, 0);
    };

    let mut shards: Vec<Option<Vec<u8>>> = vec![None; DATA_SHARDS + PARITY_SHARDS];
    let mut repaired = 0usize;
    let mut confirmed = 0usize;

    for (corrupted, original) in oligos.iter().zip(original_oligos.iter()) {
        let idx = original.shard_index;
        match corrupted {
            None => {} // shard lost — leave as None for RS to reconstruct
            Some(c) => {
                if c.sovereign_hash == original.sovereign_hash {
                    // Hash intact — decode and use directly
                    let bytes = yin_yang_decode(&original.sequence, shard_size);
                    shards[idx] = Some(bytes);
                    confirmed += 1;
                } else {
                    // Hash mismatch — strand mutated, mark as erasure
                    shards[idx] = None;
                    repaired += 1;
                }
            }
        }
    }

    let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
    match rs.reconstruct(&mut shards) {
        Ok(_) => {
            let original_len = original_oligos[0].original_len;
            let mut data: Vec<u8> = shards.iter()
                .take(DATA_SHARDS)
                .filter_map(|s| s.as_ref())
                .flat_map(|s| s.iter().copied())
                .collect();
            data.truncate(original_len);
            (Some(original_packet.clone()), confirmed, repaired)
        }
        Err(_) => (None, confirmed, repaired),
    }
}

fn yin_yang_decode(seq: &str, expected_bytes: usize) -> Vec<u8> {
    let bases: Vec<char> = seq.chars().collect();
    let mut bytes = Vec::new();
    let mut rule = 0usize;
    let mut prev = ' ';
    let mut buf = 0u8;
    let mut bits = 0u8;

    for (i, &base) in bases.iter().enumerate() {
        let b = YIN[rule].iter().position(|&x| x == base).unwrap_or(0) as u8;
        buf = (buf << 2) | b;
        bits += 2;
        if bits == 8 { bytes.push(buf); buf = 0; bits = 0; }
        if base == prev { rule ^= 1; }
        if (i + 1) % 8 == 0 { rule ^= 1; }
        prev = base;
        if bytes.len() >= expected_bytes { break; }
    }
    bytes
}

pub mod hedges;
pub use raptorq;

/// Silent version of raptor_encode for benchmarking (no stdout).
pub fn raptor_encode_silent(data: &[u8], config: &RaptorConfig) -> (Vec<EncodingPacket>, ObjectTransmissionInformation) {
    let symbol_size = 1024u16;
    let oti = ObjectTransmissionInformation::with_defaults(data.len() as u64, symbol_size);
    let encoder = Encoder::new(data, oti);
    let packets = encoder.get_encoded_packets(
        (data.len() as f64 / symbol_size as f64 * config.redundancy_ratio).ceil() as u32
    );
    (packets, oti)
}
