use std::env;

fn main() {
    let jwt = match env::var("TWIST_JWT") {
        Ok(v) => v,
        Err(_) => { eprintln!("Set TWIST_JWT env var"); std::process::exit(1); }
    };
    let eut = match env::var("TWIST_EUT") {
        Ok(v) => v,
        Err(_) => { eprintln!("Set TWIST_EUT env var"); std::process::exit(1); }
    };
    println!("Testing Twist API...");
    match sovereign_vault::twist_api::validate_sovereign_sequences(&jwt, &eut, "matthew.schoville@gmail.com") {
        Ok(()) => println!("SUCCESS"),
        Err(e) => println!("FAILED: {}", e),
    }
}
