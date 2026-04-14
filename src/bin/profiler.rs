use std::time::Instant;
use sovereign_vault::{raptor_encode_silent, RaptorConfig};
use rayon::prelude::*;

fn bench(label: &str, size_mb: f64, symbol_size_bytes: u16, runs: usize) {
    let size = (size_mb * 1_048_576.0) as usize;
    let data: Vec<u8> = (0..size)
        .map(|i: usize| i.wrapping_mul(1664525).wrapping_add(1013904223) as u8)
        .collect();

    // raptor_encode_silent uses 1024-byte symbols
    let config = RaptorConfig { redundancy_ratio: 0.30 };

    // warmup
    let _ = raptor_encode_silent(&data, &config);

    let t = Instant::now();
    for _ in 0..runs {
        let _ = raptor_encode_silent(&data, &config);
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
    let mbps = size_mb / (ms / 1000.0);
    let gbps = mbps / 1000.0;

    println!("  {:20} {:>6.0} MB  {:>8.1} ms  {:>8.1} MB/s  ({:.3} Gb/s)",
        label, size_mb, ms, mbps, gbps * 8.0);
}

fn bench_parallel(label: &str, size_mb: f64, runs: usize) {
    let size = (size_mb * 1_048_576.0) as usize;
    let data: Vec<u8> = (0..size)
        .map(|i: usize| i.wrapping_mul(1664525).wrapping_add(1013904223) as u8)
        .collect();

    let config = RaptorConfig { redundancy_ratio: 0.30 };
    let cores = rayon::current_num_threads();
    let chunk = (size / cores).max(65536);

    // warmup
    let _: Vec<_> = data.par_chunks(chunk)
        .map(|c| raptor_encode_silent(c, &config))
        .collect();

    let t = Instant::now();
    for _ in 0..runs {
        let _: Vec<_> = data.par_chunks(chunk)
            .map(|c| raptor_encode_silent(c, &config))
            .collect();
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
    let mbps = size_mb / (ms / 1000.0);

    println!("  {:20} {:>6.0} MB  {:>8.1} ms  {:>8.1} MB/s  ({:.3} Gb/s)  [{} cores]",
        label, size_mb, ms, mbps, mbps / 1000.0 * 8.0, cores);
}

fn main() {
    let cores = rayon::current_num_threads();
    println!("\n  SOVEREIGNFLOW — BOTTLENECK PROFILER");
    println!("  Symbol size: 1024 bytes | Cores: {} | Build: release\n", cores);

    println!("  {:<20} {:>6}  {:>9}  {:>10}  {:>12}",
        "Layer", "Size", "Time(ms)", "MB/s", "Gb/s (net)");
    println!("  {}", "─".repeat(68));

    // BLAKE3 — sovereign audit layer
    {
        let size = 16_777_216usize;
        let data: Vec<u8> = (0..size).map(|i| i as u8).collect();
        let t = Instant::now();
        for _ in 0..20 { let _ = blake3::hash(&data); }
        let ms = t.elapsed().as_secs_f64() * 1000.0 / 20.0;
        let mbps = 16.0 / (ms / 1000.0);
        println!("  {:20} {:>6}  {:>8.1}ms  {:>8.1} MB/s  ({:.3} Gb/s)  [baseline]",
            "BLAKE3 (audit)", 16, ms, mbps, mbps / 1000.0 * 8.0);
    }

    println!("  {}", "─".repeat(68));

    // RaptorQ at different sizes — single core
    for (mb, runs) in [(1.0, 10), (4.0, 5), (16.0, 3), (64.0, 2)] {
        bench(&format!("RaptorQ {}MB (1c)", mb as usize), mb, 1024, runs);
    }

    println!("  {}", "─".repeat(68));

    // RaptorQ parallel
    for (mb, runs) in [(1.0, 10), (4.0, 5), (16.0, 3), (64.0, 2)] {
        bench_parallel(&format!("RaptorQ {}MB ({}c)", mb as usize, cores), mb, runs);
    }

    println!("  {}", "─".repeat(68));

    // What SIMD can realistically add
    println!();
    println!("  OPTIMIZATION ROADMAP:");
    println!();

    let (_, oti) = raptor_encode_silent(
        &vec![0u8; 1_048_576],
        &RaptorConfig { redundancy_ratio: 0.30 }
    );
    // Approximate current peak from parallel 1MB (most cache-friendly)
    let current_peak_mbps = {
        let data = vec![0u8; 1_048_576];
        let config = RaptorConfig { redundancy_ratio: 0.30 };
        let cores = rayon::current_num_threads();
        let chunk = (1_048_576 / cores).max(65536);
        let t = Instant::now();
        for _ in 0..10 {
            let _: Vec<_> = data.par_chunks(chunk)
                .map(|c| raptor_encode_silent(c, &config))
                .collect();
        }
        1.0 / (t.elapsed().as_secs_f64() / 10.0) * 1000.0
    };

    println!("  Current peak (Rayon, {} cores):  {:.0} MB/s  =  {:.2} Gb/s",
        cores, current_peak_mbps, current_peak_mbps * 8.0 / 1000.0);
    println!();
    println!("  Step 1 — AVX-512 XOR (SIMD):     ~{:.0} MB/s  =  {:.2} Gb/s  (3x est.)",
        current_peak_mbps * 3.0, current_peak_mbps * 3.0 * 8.0 / 1000.0);
    println!("  Step 2 — GPU XOR offload:         ~{:.0} MB/s  =  {:.2} Gb/s  (8x est.)",
        current_peak_mbps * 8.0, current_peak_mbps * 8.0 * 8.0 / 1000.0);
    println!("  Step 3 — FPGA pipeline:           ~1250 MB/s  =  10.00 Gb/s  (target)");
    println!();
    println!("  WSL2 memory bandwidth ceiling (~25 GB/s on this hardware):");
    println!("  Your CPU can theoretically push {:.0} MB/s before hitting RAM ceiling.",
        25000.0_f64);
    println!("  Gap to FPGA target: {:.1}x", 1250.0 / current_peak_mbps.max(1.0));
    println!();

    // Honest assessment
    if current_peak_mbps > 1000.0 {
        println!("  STATUS: Already at 10 Gb/s class. SIMD = refinement only.");
    } else if current_peak_mbps > 400.0 {
        println!("  STATUS: SIMD AVX-512 can realistically reach 10 Gb/s target.");
        println!("          Implement portable_simd or std::simd on XOR inner loop.");
    } else {
        println!("  STATUS: Symbol size or packet overhead is the ceiling.");
        println!("          Increase chunk granularity before adding SIMD.");
    }
}
