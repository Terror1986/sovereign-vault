# SovereignFlow Gateway

The OS for Zero-Watt Data. A DNA storage middleware stack written in Rust.

## Architecture
## Benchmarks (4-core, release build)

| Metric | Value |
|--------|-------|
| Peak encode (4 cores) | 1,174 MB/s |
| Peak decode | 2,843 MB/s |
| Strand loss tolerance | 20% |
| Base mutation tolerance | 2% |
| Indel correction | 3 per strand |
| Recovery accuracy | 100% byte-perfect |

## Usage

```bash
# Encode any file to DNA instruction set
./target/release/vault encode myfile.pdf myfile.dna

# Decode back to original
./target/release/vault decode myfile.dna recovered.pdf

# Verify
diff myfile.pdf recovered.pdf && echo "PERFECT MATCH"

# Run benchmarks
./target/release/benchmark

# Run chaos simulation
./target/release/sovereign_vault
```

## Build

```bash
cargo build --release
```

## Stack

- `raptorq` — RFC 6330 fountain codes
- `reed-solomon-erasure` — GF(2^8) erasure coding
- `blake3` — cryptographic sovereign audit hashes
- `rayon` — parallel encoding across CPU cores

## Sovereign Infrastructure

SovereignFlow is designed for environments where data integrity is non-negotiable:

- **Air-gapped deployment** — runs fully offline, no vendor cloud required
- **Cryptographic auditability** — BLAKE3 sovereign hash embedded per strand; tamper detection is mathematically guaranteed
- **Sovereign verification** — the user owns the protocol, the keys, and the molecules; no third-party trust required
- **Zero-power long-term archive** — encoded DNA requires no electricity for storage; stable for 1,000+ years under standard conditions
- **Patent pending** — USPTO provisional 64/038,618

These properties make SovereignFlow directly applicable to:
- Government and defense archival mandates
- Financial regulatory retention requirements  
- Intelligence community sovereign data infrastructure
- Century-scale institutional memory preservation
