use sovereign_vault::hedges::hedges_encode;
use sovereign_vault::twist_api::{TwistClient, OligoSequence};
use std::env;

fn main() {
    let jwt = env::var("TWIST_JWT").expect("Set TWIST_JWT");
    let eut = env::var("TWIST_EUT").expect("Set TWIST_EUT");
    
    let client = TwistClient::new(jwt, eut, "matthew.schoville@gmail.com".to_string());

    // Diverse real data patterns -- not just repeated bytes
    let test_patterns: Vec<(&str, Vec<u8>)> = vec![
        ("all_zeros",     vec![0x00u8; 17]),
        ("all_ones",      vec![0xFFu8; 17]),
        ("alternating",   vec![0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA]),
        ("sequential",    (0u8..17).collect()),
        ("reverse",       (0u8..17).rev().collect()),
        ("random_like",   vec![0x3F, 0x7A, 0x12, 0xE4, 0x89, 0xC1, 0x56, 0x2D, 0xF0, 0x4B, 0x93, 0x6E, 0xA7, 0x1C, 0x8F, 0xD5, 0x42]),
        ("high_entropy",  vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11]),
        ("text_like",     b"SovereignFlow!!".to_vec()),
    ];

    let mut sequences: Vec<OligoSequence> = Vec::new();
    
    println!("Generating diverse 136-base SovereignFlow oligos:\n");
    for (i, (name, data)) in test_patterns.iter().enumerate() {
        let encoded = hedges_encode(data, i as u32);
        let seq = String::from_utf8(encoded.clone()).unwrap();
        println!("  {} -- {} bases -- GC: {:.0}%", 
            name, seq.len(),
            seq.chars().filter(|c| *c == 'G' || *c == 'C').count() as f64 / seq.len() as f64 * 100.0
        );
        sequences.push(OligoSequence {
            name: format!("SF-{}-{:03}", name, i),
            sequence: seq,
        });
    }

    println!("\nSubmitting {} diverse oligos to Twist synthesizability engine...", sequences.len());
    
    match client.check_synthesizability(sequences) {
        Ok(response) => {
            let construct_id = response.id.clone().unwrap_or_default();
            println!("Construct ID: {}", construct_id);
            
            std::thread::sleep(std::time::Duration::from_secs(2));
            
            match client.get_construct_scoring(&construct_id) {
                Ok(scoring) => {
                    if let Some(arr) = scoring.as_array() {
                        for item in arr {
                            let score = item["score"].as_str().unwrap_or("unknown");
                            let difficulty = item["score_data"]["difficulty"].as_str().unwrap_or("unknown");
                            let issues = &item["issues"];
                            let name = item["name"].as_str().unwrap_or("unknown");
                            let issue_count = issues.as_array().map(|a| a.len()).unwrap_or(0);
                            println!("  {} -- Score: {} -- Difficulty: {} -- Issues: {}", 
                                name, score, difficulty, issue_count);
                        }
                    }
                }
                Err(e) => println!("Scoring error: {}", e),
            }
        }
        Err(e) => println!("Failed: {}", e),
    }
}
