use std::time::Instant;
use sovereign_vault::hedges::{hedges_encode, hedges_decode};
use rayon::prelude::*;

fn main() {
    println!("\n  HEDGES THROUGHPUT BENCHMARK");
    println!("  ============================\n");

    let num_strands = 500; // smaller for fast benchmark
    let shard_bytes = 128;
    
    let data: Vec<u8> = (0..shard_bytes).map(|i| i as u8).collect();
    let strands: Vec<(Vec<u8>, u32)> = (0..num_strands as u32)
        .map(|id| (hedges_encode(&data, id), id))
        .collect();

    println!("Sequential ({} strands x {} bytes):", num_strands, shard_bytes);
    let seq_start = Instant::now();
    let _: Vec<Vec<u8>> = strands.iter()
        .map(|(s, id)| hedges_decode(s, shard_bytes, *id).0)
        .collect();
    let seq_time = seq_start.elapsed();
    let seq_mbs = (num_strands * shard_bytes) as f64 / seq_time.as_secs_f64() / 1_048_576.0;
    println!("  {:.2} MB/s  ({:.3} Gb/s)  [{:.2?}]", seq_mbs, seq_mbs * 8.0 / 1000.0, seq_time);

    println!("\nParallel (Rayon {} threads):", rayon::current_num_threads());
    let par_start = Instant::now();
    let _: Vec<Vec<u8>> = strands.par_iter()
        .map(|(s, id)| hedges_decode(s, shard_bytes, *id).0)
        .collect();
    let par_time = par_start.elapsed();
    let par_mbs = (num_strands * shard_bytes) as f64 / par_time.as_secs_f64() / 1_048_576.0;
    println!("  {:.2} MB/s  ({:.3} Gb/s)  [{:.2?}]", par_mbs, par_mbs * 8.0 / 1000.0, par_time);

    let speedup = seq_time.as_secs_f64() / par_time.as_secs_f64();
    println!("\n  Speedup: {:.2}x", speedup);
    println!("  Projected full pipeline: {:.2} Gb/s", 0.96 * speedup);
}
