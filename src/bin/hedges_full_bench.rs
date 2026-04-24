use std::time::Instant;
use rayon::prelude::*;
use sovereign_vault::hedges::{hedges_encode, hedges_decode};

fn main() {
    println!("\n  SOVEREIGNFLOW FULL HEDGES PIPELINE BENCHMARK");
    println!("  =============================================\n");

    let cores = rayon::current_num_threads();
    println!("  CPU cores: {}", cores);

    // Realistic strand parameters
    let shard_bytes = 17usize;  // 17 bytes * 8 bits = 136 bases per strand
    let num_strands = 50_000usize;  // large enough for meaningful measurement
    let total_bytes = shard_bytes * num_strands;

    println!("  Strands: {}", num_strands);
    println!("  Bytes per strand: {}", shard_bytes);
    println!("  Total data: {:.1} MB\n", total_bytes as f64 / 1_048_576.0);

    // Generate test data
    let data: Vec<Vec<u8>> = (0..num_strands)
        .map(|i| (0..shard_bytes).map(|j| ((i * 7 + j * 13) % 256) as u8).collect())
        .collect();

    // ENCODE -- parallel
    println!("Encoding {} strands...", num_strands);
    let enc_start = Instant::now();
    let encoded: Vec<Vec<u8>> = data.par_iter()
        .enumerate()
        .map(|(i, d)| hedges_encode(d, i as u32))
        .collect();
    let enc_time = enc_start.elapsed();
    let enc_mbs = (total_bytes as f64 / enc_time.as_secs_f64()) / 1_048_576.0;
    println!("  Time: {:.2?}  Throughput: {:.2} MB/s ({:.3} Gb/s)",
        enc_time, enc_mbs, enc_mbs * 8.0 / 1000.0);

    // DECODE sequential
    println!("\nDecoding {} strands (sequential)...", num_strands);
    let seq_start = Instant::now();
    let _seq: Vec<Vec<u8>> = encoded.iter()
        .enumerate()
        .map(|(i, e)| hedges_decode(e, shard_bytes, i as u32).0)
        .collect();
    let seq_time = seq_start.elapsed();
    let seq_mbs = (total_bytes as f64 / seq_time.as_secs_f64()) / 1_048_576.0;
    println!("  Time: {:.2?}  Throughput: {:.2} MB/s ({:.3} Gb/s)",
        seq_time, seq_mbs, seq_mbs * 8.0 / 1000.0);

    // DECODE parallel
    println!("\nDecoding {} strands (Rayon {} threads)...", num_strands, cores);
    let par_start = Instant::now();
    let par_decoded: Vec<Vec<u8>> = encoded.par_iter()
        .enumerate()
        .map(|(i, e)| hedges_decode(e, shard_bytes, i as u32).0)
        .collect();
    let par_time = par_start.elapsed();
    let par_mbs = (total_bytes as f64 / par_time.as_secs_f64()) / 1_048_576.0;
    println!("  Time: {:.2?}  Throughput: {:.2} MB/s ({:.3} Gb/s)",
        par_time, par_mbs, par_mbs * 8.0 / 1000.0);

    // Verify correctness
    let correct = par_decoded.iter().zip(data.iter())
        .filter(|(d, o)| *d == *o)
        .count();
    
    let speedup = seq_time.as_secs_f64() / par_time.as_secs_f64();
    println!("\n  Speedup: {:.2}x", speedup);
    println!("  Correct: {}/{} strands", correct, num_strands);
    println!("  Accuracy: {}%", if correct == num_strands { "100" } else { "<100" });
    println!("\n  Sequential HEDGES: {:.3} Gb/s", seq_mbs * 8.0 / 1000.0);
    println!("  Parallel HEDGES:   {:.3} Gb/s", par_mbs * 8.0 / 1000.0);
}
