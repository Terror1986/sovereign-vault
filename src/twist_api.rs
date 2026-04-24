//! Twist Bioscience TAPI Integration
//!
//! Submits SovereignFlow-encoded ATGC sequences to Twist for synthesis.
//! Validates synthesizability before ordering.
//! 
//! Usage:
//!   Set environment variables:
//!     TWIST_JWT_TOKEN=your_jwt_token
//!     TWIST_END_USER_TOKEN=your_end_user_token
//!     TWIST_EMAIL=matthew.schoville@gmail.com

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, ACCEPT};
use serde::{Deserialize, Serialize};

const TWIST_API_BASE: &str = "https://twist-api.twistdna.com/v1";

pub struct TwistClient {
    client: Client,
    jwt_token: String,
    end_user_token: String,
    email: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OligoSequence {
    pub name: String,
    pub sequence: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SynthesisabilityRequest {
    pub sequences: Vec<OligoSequence>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SynthesisabilityResult {
    pub name: String,
    pub sequence: String,
    pub synthesizable: Option<bool>,
    pub score: Option<f64>,
    pub warnings: Option<Vec<String>>,
    pub errors: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SynthesisabilityResponse {
    pub results: Option<Vec<SynthesisabilityResult>>,
}

// Constructs API -- per Tiffany Dai, Twist TAPI support
#[derive(Serialize, Deserialize, Debug)]
pub struct ConstructRequest {
    pub sequences: Vec<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub construct_type: String,
    pub vector_mes_uid: String,
    pub insertion_point_mes_uid: String,
    pub notes: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConstructResponse {
    pub id: Option<String>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub scoring: Option<serde_json::Value>,
}

impl TwistClient {
    pub fn new(jwt_token: String, end_user_token: String, email: String) -> Self {
        TwistClient {
            client: Client::new(),
            jwt_token,
            end_user_token,
            email,
        }
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("JWT {}", self.jwt_token)).unwrap(),
        );
        headers.insert(
            "X-End-User-Token",
            HeaderValue::from_str(&self.end_user_token).unwrap(),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers
    }

    /// Test API connectivity -- verify credentials and IP whitelist
    pub fn test_connection(&self) -> Result<String, String> {
        let url = format!("{}/users/{}/", TWIST_API_BASE, self.email);
        let response = self.client
            .get(&url)
            .headers(self.headers())
            .send()
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();
        let body = response.text().unwrap_or_default();

        if status.is_success() {
            Ok(format!("Connected successfully: {}", body))
        } else {
            Err(format!("Connection failed: {} -- {}", status, body))
        }
    }

    /// Check synthesizability via /constructs endpoint -- per Tiffany Dai, Twist TAPI
    /// Uses OLIGO_POOL type as confirmed by Twist support
    pub fn check_synthesizability(
        &self,
        sequences: Vec<OligoSequence>,
    ) -> Result<ConstructResponse, String> {
        let url = format!("{}/users/{}/constructs/", TWIST_API_BASE, self.email);
        
        let seq_strings: Vec<String> = sequences.iter()
            .map(|s| s.sequence.clone())
            .collect();
            
        let payload = ConstructRequest {
            sequences: seq_strings,
            name: "SF-synth-check".to_string(),
            construct_type: "OLIGO_POOL".to_string(),
            vector_mes_uid: "na".to_string(),
            insertion_point_mes_uid: "na".to_string(),
            notes: "SovereignFlow DNA storage codec synthesizability validation".to_string(),
        };

        let response = self.client
            .post(&url)
            .headers(self.headers())
            .json(&payload)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();
        let body = response.text().unwrap_or_default();

        if status.is_success() {
            let parsed: ConstructResponse = serde_json::from_str(&body)
                .map_err(|e| format!("Parse failed: {} -- body: {}", e, body))?;
            Ok(parsed)
        } else {
            Err(format!("Construct creation failed: {} -- {}", status, body))
        }
    }
}

/// Encode test data through SovereignFlow and check synthesizability
pub fn validate_sovereign_sequences(
    jwt_token: &str,
    end_user_token: &str,
    email: &str,
) -> Result<(), String> {
    let client = TwistClient::new(
        jwt_token.to_string(),
        end_user_token.to_string(),
        email.to_string(),
    );

    println!("Testing Twist API connection...");
    match client.test_connection() {
        Ok(msg) => println!("✅ {}", msg),
        Err(e) => return Err(format!("Connection failed: {}", e)),
    }

    // Test sequences -- these would come from SovereignFlow encoder in production
    // Using short test sequences that meet biological constraints
    let test_sequences = vec![
        OligoSequence {
            name: "SovereignFlow_test_001".to_string(),
            sequence: "GCATATCGCTCTATCTATAGCGATGTCTACAGCGAGACGTACATATATGTAGCTCGCGAT".to_string(),
        },
        OligoSequence {
            name: "SovereignFlow_test_002".to_string(),
            sequence: "ATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG".to_string(),
        },
    ];

    println!("\nChecking synthesizability of {} test sequences...", test_sequences.len());
    match client.check_synthesizability(test_sequences) {
        Ok(response) => {
            println!("  ✅ Construct created successfully");
            if let Some(id) = &response.id {
                println!("  Construct ID: {}", id);
            }
            if let Some(name) = &response.name {
                println!("  Name: {}", name);
            }
            if let Some(status) = &response.status {
                println!("  Status: {}", status);
            }
            if let Some(scoring) = &response.scoring {
                println!("  Scoring: {}", scoring);
            }
            Ok(())
        }
        Err(e) => Err(format!("Synthesizability check failed: {}", e)),
    }
}
