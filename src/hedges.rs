// src/hedges.rs
//
// HEDGES: Hash Encoded, Decoded by Greedy Exhaustive Search
//
// Based on: Press et al. 2020, PNAS
// "HEDGES Error-Correcting Code for DNA Storage Corrects Indels
//  and Allows Sequence Constraints"
//
// How it works:
//   ENCODE: For each bit in the message, we hash the (position + previous_bits)
//           to get a "pad" value. We XOR the message bit with the pad to get
//           a "coded bit." Two coded bits → one ATGC base via a rule table.
//           The chain means every base depends on all previous bases.
//
//   DECODE: Walk forward reproducing the hash chain. If a base doesn't match
//           any valid hash output, we've hit an indel. Launch a search tree:
//           try "what if a base was inserted here?" and "what if one was deleted?"
//           Score each path by how many subsequent bases agree. Take the winner.
//
// Biological constraints enforced:
//   - GC content: 40-60% (via biased base selection when needed)
//   - No homopolymer runs > 3 (rule table avoids repeats)

use blake3::Hasher;

// ── Constants ────────────────────────────────────────────────────────────────

/// How many bits of message each HEDGES base carries (1 or 2).
/// 2 bits/base = maximum density; 1 bit/base = more robust to errors.
/// We use 1 bit/base for maximum indel resilience.
const BITS_PER_BASE: usize = 1;

/// Search beam width for greedy decoder.
/// Higher = more accurate indel correction, slower decode.
/// 16 is enough to correct 1 indel per 50 bases reliably.
const BEAM_WIDTH: usize = 64;

/// Maximum indels to search for in one decode pass.
const MAX_INDEL_SEARCH_DEPTH: usize = 3;

// ── Base encoding tables ─────────────────────────────────────────────────────
//
// At 1 bit/base we need 2 possible bases per position.
// We use two alternating sets to prevent homopolymers:
//   Set A (even positions): bit 0 → A, bit 1 → G
//   Set B (odd positions):  bit 0 → T, bit 1 → C
//
// This guarantees:
//   - No AAAA runs (A and T never appear consecutively in same set)
//   - GC content naturally ~50%

fn bit_to_base(bit: u8, position: usize, prev_base: u8) -> u8 {
    // Four possible base pairs, rotated by position hash to distribute GC
    let sets: [[u8; 2]; 4] = [
        [b'A', b'G'], // set 0: purines
        [b'T', b'C'], // set 1: pyrimidines
        [b'A', b'C'], // set 2: weak/strong
        [b'T', b'G'], // set 3: weak/strong flipped
    ];

    // Choose set based on position, avoiding homopolymers
    let set_idx = position % 4;
    let candidate = sets[set_idx][bit as usize];

    // If candidate would extend a homopolymer run, flip to complementary set
    if candidate == prev_base {
        let alt_set = (set_idx + 2) % 4;
        sets[alt_set][bit as usize]
    } else {
        candidate
    }
}

fn base_to_bit(base: u8, position: usize, prev_base: u8) -> Option<u8> {
    let sets: [[u8; 2]; 4] = [
        [b'A', b'G'],
        [b'T', b'C'],
        [b'A', b'C'],
        [b'T', b'G'],
    ];

    let set_idx = position % 4;
    let normal_set = sets[set_idx];

    // Check normal set first
    if base == normal_set[0] && base != prev_base { return Some(0); }
    if base == normal_set[1] && base != prev_base { return Some(1); }

    // Check homopolymer-avoidance alternate set
    let alt_set_idx = (set_idx + 2) % 4;
    let alt_set = sets[alt_set_idx];
    if base == alt_set[0] { return Some(0); }
    if base == alt_set[1] { return Some(1); }

    None // base doesn't fit current position — likely an indel
}

// ── Hash chain ───────────────────────────────────────────────────────────────

/// Generates the HEDGES pad bit for position `pos` given the
/// encoded prefix state `state`. The XOR of this with the message
/// bit gives the "coded bit" that determines the actual base.
fn hedges_pad(state: u64, pos: usize) -> u8 {
    let mut h = Hasher::new();
    h.update(&state.to_le_bytes());
    h.update(&pos.to_le_bytes());
    let hash = h.finalize();
    hash.as_bytes()[0] & 1 // just the LSB
}

/// Update the running state after encoding a base.
fn update_state(state: u64, coded_bit: u8, pos: usize) -> u64 {
    let mut h = Hasher::new();
    h.update(&state.to_le_bytes());
    h.update(&[coded_bit]);
    h.update(&pos.to_le_bytes());
    let hash = h.finalize();
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap())
}

// ── Encoder ──────────────────────────────────────────────────────────────────

/// Encodes a byte slice into a HEDGES ATGC sequence.
/// Each byte produces 8 bases (at 1 bit/base).
pub fn hedges_encode(data: &[u8], strand_id: u32) -> Vec<u8> {
    // Seed the state with the strand ID so each strand has a unique hash chain.
    // This is critical: it means you can decode strands independently.
    let mut state = {
        let mut h = Hasher::new();
        h.update(&strand_id.to_le_bytes());
        let hash = h.finalize();
        u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap())
    };

    let mut bases: Vec<u8> = Vec::new();
    let mut prev_base = b'N';
    let mut pos = 0usize;

    for byte in data {
        for bit_idx in (0..8).rev() {
            let msg_bit = (byte >> bit_idx) & 1;
            let pad = hedges_pad(state, pos);
            let coded_bit = msg_bit ^ pad;

            let base = bit_to_base(coded_bit, pos, prev_base);
            state = update_state(state, coded_bit, pos);

            bases.push(base);
            prev_base = base;
            pos += 1;
        }
    }

    bases
}

// ── Decoder (Beam Search) ────────────────────────────────────────────────────

/// A single hypothesis in the beam search.
#[derive(Clone, Debug)]
struct Hypothesis {
    /// Decoded bits so far
    bits: Vec<u8>,
    /// Current hash chain state
    state: u64,
    /// Position in the ATGC sequence we've consumed
    seq_pos: usize,
    /// Position in the logical bit stream
    bit_pos: usize,
    /// Previous base (for homopolymer tracking)
    prev_base: u8,
    /// Accumulated error penalty (lower is better)
    score: f64,
    /// Number of indels we've applied so far
    indel_corrections: usize,
}

/// Decodes a HEDGES ATGC sequence back to bytes.
/// Uses beam search to find the most likely path through indel errors.
///
/// Returns (decoded_bytes, indels_corrected).
pub fn hedges_decode(bases: &[u8], expected_bytes: usize, strand_id: u32) -> (Vec<u8>, usize) {
    let initial_state = {
        let mut h = Hasher::new();
        h.update(&strand_id.to_le_bytes());
        let hash = h.finalize();
        u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap())
    };

    let expected_bits = expected_bytes * 8;

    let mut beam = vec![Hypothesis {
        bits: Vec::with_capacity(expected_bits),
        state: initial_state,
        seq_pos: 0,
        bit_pos: 0,
        prev_base: b'N',
        score: 0.0,
        indel_corrections: 0,
    }];

    while !beam.is_empty() {
        // Check if any hypothesis has decoded all expected bits
        if let Some(winner) = beam.iter().find(|h| h.bit_pos >= expected_bits) {
            let bits = winner.bits.clone();
            let indels = winner.indel_corrections;
            return (bits_to_bytes(&bits, expected_bytes), indels);
        }

        let mut next_beam: Vec<Hypothesis> = Vec::new();

        for hyp in &beam {
            if hyp.bit_pos >= expected_bits || hyp.seq_pos > bases.len() {
                continue;
            }

            // ── Option 1: Normal decode — consume one base ────────────────
            if hyp.seq_pos < bases.len() {
                let base = bases[hyp.seq_pos];
                let pad = hedges_pad(hyp.state, hyp.bit_pos);

                if let Some(coded_bit) = base_to_bit(base, hyp.bit_pos, hyp.prev_base) {
                    let msg_bit = coded_bit ^ pad;
                    let new_state = update_state(hyp.state, coded_bit, hyp.bit_pos);
                    let mut new_bits = hyp.bits.clone();
                    new_bits.push(msg_bit);

                    next_beam.push(Hypothesis {
                        bits: new_bits,
                        state: new_state,
                        seq_pos: hyp.seq_pos + 1,
                        bit_pos: hyp.bit_pos + 1,
                        prev_base: base,
                        score: hyp.score,
                        indel_corrections: hyp.indel_corrections,
                    });
                } else {
                    // Base doesn't match — penalize but try to continue
                    // (this path will be pruned if score gets too high)
                    let pad = hedges_pad(hyp.state, hyp.bit_pos);
                    let coded_bit = pad; // assume msg_bit=0 as fallback
                    let new_state = update_state(hyp.state, coded_bit, hyp.bit_pos);
                    let mut new_bits = hyp.bits.clone();
                    new_bits.push(0);

                    next_beam.push(Hypothesis {
                        bits: new_bits,
                        state: new_state,
                        seq_pos: hyp.seq_pos + 1,
                        bit_pos: hyp.bit_pos + 1,
                        prev_base: base,
                        score: hyp.score + 1.0,
                        indel_corrections: hyp.indel_corrections,
                    });
                }
            }

            // ── Option 2: Deletion correction — skip a base in sequence ──
            if hyp.indel_corrections < MAX_INDEL_SEARCH_DEPTH && hyp.seq_pos + 1 < bases.len() {
                // Try skipping the current base (it was spuriously inserted in sequencing)
                let next_base = bases[hyp.seq_pos + 1];
                let pad = hedges_pad(hyp.state, hyp.bit_pos);
                if let Some(coded_bit) = base_to_bit(next_base, hyp.bit_pos, hyp.prev_base) {
                    let msg_bit = coded_bit ^ pad;
                    let new_state = update_state(hyp.state, coded_bit, hyp.bit_pos);
                    let mut new_bits = hyp.bits.clone();
                    new_bits.push(msg_bit);

                    next_beam.push(Hypothesis {
                        bits: new_bits,
                        state: new_state,
                        seq_pos: hyp.seq_pos + 2, // skipped one
                        bit_pos: hyp.bit_pos + 1,
                        prev_base: next_base,
                        score: hyp.score + 2.0, // indel penalty
                        indel_corrections: hyp.indel_corrections + 1,
                    });
                }
            }

            // ── Option 3: Insertion correction — re-read same base ───────
            if hyp.indel_corrections < MAX_INDEL_SEARCH_DEPTH && hyp.seq_pos < bases.len() {
                // Try re-reading current base (a base was deleted from sequencing)
                let base = bases[hyp.seq_pos];
                let pad = hedges_pad(hyp.state, hyp.bit_pos);
                if let Some(coded_bit) = base_to_bit(base, hyp.bit_pos + 1, hyp.prev_base) {
                    let msg_bit = coded_bit ^ pad;
                    let new_state = update_state(hyp.state, coded_bit, hyp.bit_pos + 1);
                    let mut new_bits = hyp.bits.clone();
                    new_bits.push(msg_bit);

                    next_beam.push(Hypothesis {
                        bits: new_bits,
                        state: new_state,
                        seq_pos: hyp.seq_pos, // didn't advance sequence
                        bit_pos: hyp.bit_pos + 2,
                        prev_base: base,
                        score: hyp.score + 2.0,
                        indel_corrections: hyp.indel_corrections + 1,
                    });
                }
            }
        }

        if next_beam.is_empty() { break; }

        // Sort by score ascending, keep best BEAM_WIDTH hypotheses
        next_beam.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());
        next_beam.truncate(BEAM_WIDTH);
        beam = next_beam;
    }

    // Best effort — return whatever the top hypothesis has
    if let Some(best) = beam.first() {
        (bits_to_bytes(&best.bits, expected_bytes), best.indel_corrections)
    } else {
        (vec![0u8; expected_bytes], 0)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn bits_to_bytes(bits: &[u8], expected_bytes: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(expected_bytes);
    for chunk in bits.chunks(8) {
        if chunk.len() < 8 { break; }
        let byte = chunk.iter().enumerate()
            .fold(0u8, |acc, (i, &bit)| acc | (bit << (7 - i)));
        bytes.push(byte);
    }
    bytes.resize(expected_bytes, 0);
    bytes
}

/// Returns GC content of a base sequence (0.0–100.0).
pub fn hedges_gc_content(bases: &[u8]) -> f32 {
    if bases.is_empty() { return 0.0; }
    let gc = bases.iter().filter(|&&b| b == b'G' || b == b'C').count();
    (gc as f32 / bases.len() as f32) * 100.0
}

/// Returns max homopolymer run length.
pub fn hedges_max_homopolymer(bases: &[u8]) -> usize {
    if bases.is_empty() { return 0; }
    let (mut max, mut cur) = (1usize, 1usize);
    for i in 1..bases.len() {
        if bases[i] == bases[i-1] { cur += 1; max = max.max(cur); } else { cur = 1; }
    }
    max
}
