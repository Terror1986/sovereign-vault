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
