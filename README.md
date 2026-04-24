Give me the full README content to paste into GitHub:

---

> **PATENT PENDING -- USPTO Application 64/038,618 Filed April 14, 2026.**
> All methods and systems described herein are protected under US patent law. Commercial use without a license is prohibited.

# SovereignFlow Gateway

The S3-compatible software layer for biological data storage. Written in Rust. Triple-layer ECC. Production speed. Hardware agnostic.

## Benchmarks (4-core CPU, release build)

| Metric | Value |
|--------|-------|
| Peak encode (4 cores) | 1,251 MB/s (10.0 Gb/s) |
| Full pipeline decode (CPU) | 119.5 MB/s (0.96 Gb/s) |
| GPU XOR throughput (RTX 3080 Ti, in-VRAM) | 87 GB/s (43x CPU) |
| Projected full pipeline decode (GPU) | 20-40 Gb/s |
| Projected full pipeline decode (H100) | 60-120 Gb/s |
| Code rate conservative | 0.513 bits/base |
| Code rate tunable | 0.513 to 1.087 bits/base |
| Strand loss tolerance | 40% deletion rate |
| Base mutation tolerance | 2% substitution rate |
| Indel correction | HEDGES beam search W=128 per strand |
| Recovery accuracy | 100% byte-perfect at 1GB scale |
| Index scale | 1TB+ via RocksDB persistent backend |
| vs HEDGES paper (Press 2020) | Matches at 0.513 + RaptorQ pool recovery |
| vs Gungnir (Nature 2026) | 0.513 vs ~0.4 bits/base + S3 gateway + 10 Gb/s encode |

## GPU Acceleration

RaptorQ XOR operations validated at 87 GB/s on RTX 3080 Ti with data resident in VRAM. This represents the foundational GPU primitive for full pipeline acceleration.

Architecture: upload strand pool once to VRAM, process all repair operations entirely in GPU memory, download recovered data once. Eliminates transfer overhead for large workloads.

| Buffer Size | GPU Throughput | vs CPU |
|-------------|---------------|--------|
| 10 MB | 100 GB/s | 50x |
| 100 MB | 85 GB/s | 43x |
| 500 MB | 88 GB/s | 44x |

VRAM upload speed: 7-8 GB/s. A 1GB strand pool loads in 140ms -- once per job.

Full pipeline GPU decode including HEDGES beam search parallelization is the next engineering milestone. Conservative projection: 20-40 Gb/s on RTX 3080 Ti, 60-120 Gb/s on H100.

## Architecture

Triple-layer ECC pipeline:

    S3 Request
        |
        v
    RaptorQ (RFC 6330 fountain code, 30% redundancy)
        |
        v
    Reed-Solomon GF(2^8) (4+2 shards, substitution correction)
        |
        v
    HEDGES (beam search W=128, indel correction per strand)
        |
        v
    ATGC Synthesis Instructions
        |
        v
    Sovereign Audit Index (BLAKE3 per strand, RocksDB persistent)

## Configuration

All parameters tunable via sovereign.toml without code changes. Tuning to hardware-specific error profiles moves code rate from 0.513 to 1.087 bits/base.

## Usage

    ./target/release/vault encode myfile.pdf myfile.dna
    ./target/release/vault decode myfile.dna recovered.pdf
    ./target/release/benchmark
    ./target/release/sovereign_vault
    ./target/release/total_recall
    TWIST_JWT="token" TWIST_EUT="token" ./target/release/twist_test
    ./target/release/gpu_bench

## Build

    cargo build --release

Dependencies: libzstd-dev clang llvm required for RocksDB. CUDA 12.6+ required for GPU acceleration.

## Stack

| Crate | Purpose |
|-------|---------|
| raptorq | RFC 6330 fountain codes -- erasure recovery |
| reed-solomon-erasure | GF(2^8) substitution correction |
| blake3 | Cryptographic sovereign audit hashes |
| rayon | Parallel encoding across CPU cores |
| rocksdb | Persistent 1TB+ sovereign index |
| cudarc | CUDA GPU acceleration |
| reqwest | Twist Bioscience TAPI integration |
| tokio / axum | Async S3-compatible gateway |

## Integrations

Twist Bioscience TAPI -- Active API integration for programmatic synthesis ordering and synthesizability validation.

## Sovereign Infrastructure

- Air-gapped deployment -- runs fully offline, no vendor cloud required
- Cryptographic auditability -- BLAKE3 sovereign hash per strand
- Hardware agnostic -- configurable for any synthesis platform via TOML
- 1TB+ production index -- RocksDB persistent backend
- Zero-power archive -- stable 1,000+ years
- Patent pending -- USPTO provisional 64/038,618

Applications:
- Government and defense archival mandates
- Financial regulatory retention requirements
- Intelligence community sovereign data infrastructure
- Century-scale institutional memory preservation
- Enterprise S3-compatible DNA storage gateway

## Competitive Position

| System | Code Rate | Encode | Decode | S3 | GPU | Index |
|--------|-----------|--------|--------|-----|-----|-------|
| SovereignFlow | 0.513-1.087 | 10.0 Gb/s | 0.96 Gb/s (20-40 GPU) | Yes | Yes | RocksDB |
| HEDGES 2020 | ~0.5 | Academic | Academic | No | No | No |
| Gungnir 2026 | ~0.4 | Substantial compute | Substantial compute | No | No | No |

---

*Built in Rust. Patent pending. 45 minutes from Atlas Data Storage, South San Francisco.*

---

Copy and paste that into the GitHub editor and commit.
