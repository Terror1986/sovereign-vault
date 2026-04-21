//! Twist TAPI Connection Test
//! 
//! Tests API connectivity and sequence synthesizability.
//! 
//! Usage:
//!   TWIST_JWT=your_jwt TWIST_EUT=your_end_user_token cargo run --bin twist_test

fn main() {
    let jwt = std::env::var("TWIST_JWT").unwrap_or_else(|_| {
        eprintln!("Set TWIST_JWT environment variable");
        std::process::exit(1);
    });
    
    let eut = std::env::var("TWIST_EUT").unwrap_or_else(|_| {
        eprintln!("Set TWIST_EUT environment variable");
        std::process::exit(1);
    });

    let email = "matthew.schoville@gmail.com";

    println!("\n  SOVEREIGNFLOW — TWIST TAPI VALIDATION");
    println!("  =======================================\n");

    match sovereign_vault::twist_api::validate_sovereign_sequences(&jwt, &eut, email) {
        Ok(()) => println!("\n✅ Twist integration validated successfully"),
        Err(e) => println!("\n❌ Validation failed: {}", e),
    }
}
