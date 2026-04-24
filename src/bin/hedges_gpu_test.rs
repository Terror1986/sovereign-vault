use sovereign_vault::hedges_gpu::HedgesGpu;
use sovereign_vault::hedges::{hedges_encode, hedges_decode};
use std::time::Instant;

fn main() {
    println!("\n  SOVEREIGNFLOW GPU HEDGES DECODER TEST");
    println!("  =======================================\n");

    let gpu = HedgesGpu::new().expect("GPU HEDGES init failed");
    println!("GPU: {}\n", gpu.device_name());

    // Test with simple known data
    let test_cases = vec![
        (b"Hello DNA!".to_vec(), 0u32),
        (b"SovereignFlow".to_vec(), 1u32),
        (vec![0xAA, 0x55, 0xFF, 0x00, 0x42], 2u32),
    ];

    println!("Single strand correctness test:");
    for (data, strand_id) in &test_cases {
        // Encode with CPU HEDGES
        let encoded = hedges_encode(data, *strand_id);
        
        // Decode with CPU for reference
        let (cpu_decoded, cpu_indels) = hedges_decode(&encoded, data.len(), *strand_id);
        
        // Decode with GPU
        let strands = vec![encoded.clone()];
        let ids = vec![*strand_id];
        match gpu.decode_strands(&strands, data.len(), &ids) {
            Ok((decoded, indels)) => {
                let matches_cpu = decoded[0] == cpu_decoded;
                let matches_original = decoded[0] == *data;
                println!("  Strand {}: CPU indels={} GPU indels={} matches_cpu={} matches_original={}",
                    strand_id, cpu_indels, indels[0], matches_cpu, matches_original);
            }
            Err(e) => println!("  Strand {}: GPU decode failed: {}", strand_id, e),
        }
    }

    println!("\nThroughput test -- 7992 strands simultaneously:");
    let num_strands = 7992;
    let data = vec![0xAB_u8; 16]; // 16 bytes per strand
    let strand_id = 42u32;
    let encoded = hedges_encode(&data, strand_id);
    
    let strands: Vec<Vec<u8>> = (0..num_strands).map(|_| encoded.clone()).collect();
    let ids: Vec<u32> = (0..num_strands as u32).collect();

    let start = Instant::now();
    match gpu.decode_strands(&strands, data.len(), &ids) {
        Ok((decoded, _)) => {
            let elapsed = start.elapsed();
            let total_bytes = num_strands * data.len();
            let throughput_mbs = (total_bytes as f64 / elapsed.as_secs_f64()) / 1_048_576.0;
            let throughput_gbs = throughput_mbs * 8.0 / 1000.0;
            let correct = decoded.iter().filter(|d| *d == &data).count();
            println!("  Strands: {}", num_strands);
            println!("  Correct: {}/{}", correct, num_strands);
            println!("  Time: {:.2?}", elapsed);
            println!("  Throughput: {:.2} MB/s ({:.3} Gb/s)", throughput_mbs, throughput_gbs);
        }
        Err(e) => println!("  GPU batch decode failed: {}", e),
    }
}
