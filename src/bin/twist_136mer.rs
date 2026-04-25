use sovereign_vault::hedges::hedges_encode;
use sovereign_vault::twist_api::{TwistClient, OligoSequence};
use std::env;

fn main() {
    let jwt = env::var("TWIST_JWT").expect("Set TWIST_JWT");
    let eut = env::var("TWIST_EUT").expect("Set TWIST_EUT");
    
    let client = TwistClient::new(jwt, eut, "matthew.schoville@gmail.com".to_string());

    // Generate real 136-base SovereignFlow oligos
    let data = vec![0xAB_u8; 17]; // 17 bytes = 136 bases
    let sequences: Vec<OligoSequence> = (0..5u32)
        .map(|id| {
            let encoded = hedges_encode(&data, id);
            let seq = String::from_utf8(encoded).unwrap();
            println!("Strand {}: {} ({} bases)", id, &seq[..20], seq.len());
            OligoSequence {
                name: format!("SF-136mer-strand-{:03}", id),
                sequence: seq,
            }
        })
        .collect();

    println!("\nSubmitting {} real 136-base SovereignFlow oligos to Twist...", sequences.len());
    
    match client.check_synthesizability(sequences) {
        Ok(response) => {
            let construct_id = response.id.clone().unwrap_or_default();
            println!("Construct ID: {}", construct_id);
            
            // Get scoring
            println!("\nRetrieving scoring...");
            match client.get_construct_scoring(&construct_id) {
                Ok(scoring) => {
                    if let Some(arr) = scoring.as_array() {
                        for item in arr {
                            println!("\nScore: {}", item["score"].as_str().unwrap_or("unknown"));
                            println!("Difficulty: {}", item["score_data"]["difficulty"].as_str().unwrap_or("unknown"));
                            println!("Issues: {}", item["issues"]);
                            println!("Scored: {}", item["scored"]);
                        }
                    }
                }
                Err(e) => println!("Scoring error: {}", e),
            }
        }
        Err(e) => println!("Failed: {}", e),
    }
}
