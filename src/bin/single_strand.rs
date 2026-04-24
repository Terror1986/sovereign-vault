use std::time::Instant;
use sovereign_vault::hedges::{hedges_encode, hedges_decode};

fn main() {
    let data = vec![0xABu8; 17];
    let strand_id = 42u32;
    
    let encoded = hedges_encode(&data, strand_id);
    println!("Strand length: {} bases", encoded.len());
    
    // Time single decode
    let start = Instant::now();
    let (decoded, indels) = hedges_decode(&encoded, data.len(), strand_id);
    let elapsed = start.elapsed();
    
    println!("Single decode time: {:.2?}", elapsed);
    println!("Indels corrected: {}", indels);
    println!("Correct: {}", decoded == data);
    
    // Time 100 decodes
    let start = Instant::now();
    for _ in 0..100 {
        let _ = hedges_decode(&encoded, data.len(), strand_id);
    }
    let elapsed = start.elapsed();
    println!("100 decodes: {:.2?} ({:.2?} each)", elapsed, elapsed / 100);
}
