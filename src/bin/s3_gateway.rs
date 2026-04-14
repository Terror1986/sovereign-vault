//! SovereignFlow S3-Compatible Gateway (axum 0.8)
//!
//! aws --endpoint-url http://localhost:8333 s3 cp file.pdf s3://bucket/file.pdf
//! aws --endpoint-url http://localhost:8333 s3 cp s3://bucket/file.pdf out.pdf

use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, head, put},
    body::Bytes,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObjectMeta {
    key: String,
    bucket: String,
    size: usize,
    etag: String,
    last_modified: String,
    dna_file: PathBuf,
    content_type: String,
}

#[derive(Debug, Default)]
struct Store {
    objects: HashMap<String, HashMap<String, ObjectMeta>>,
    buckets: HashMap<String, String>,
}

#[derive(Clone)]
struct AppState {
    store: Arc<RwLock<Store>>,
    storage_dir: String,
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "8333".to_string());
    let storage_dir = std::env::var("STORAGE_DIR")
        .unwrap_or_else(|_| "./dna_store".to_string());

    std::fs::create_dir_all(&storage_dir).expect("Cannot create storage dir");

    let state = AppState {
        store: Arc::new(RwLock::new(Store::default())),
        storage_dir,
    };

    println!("\n  ╔══════════════════════════════════════════════════════════╗");
    println!("  ║   SOVEREIGNFLOW S3-COMPATIBLE GATEWAY                   ║");
    println!("  ║   Endpoint:  http://localhost:{}                       ║", port);
    println!("  ║   Storage:   ./dna_store/                               ║");
    println!("  ╠══════════════════════════════════════════════════════════╣");
    println!("  ║   curl -X PUT http://localhost:{}/mybucket             ║", port);
    println!("  ║   curl -X PUT http://localhost:{}/mybucket/file \\     ║", port);
    println!("  ║        --data-binary @file.pdf                          ║");
    println!("  ║   curl http://localhost:{}/mybucket/file -o out.pdf   ║", port);
    println!("  ╚══════════════════════════════════════════════════════════╝\n");

    let app = Router::new()
        .route("/",                  get(list_buckets))
        .route("/{bucket}",           put(create_bucket))
        .route("/{bucket}",           get(list_objects))
        .route("/{bucket}/{key}",      put(put_object))
        .route("/{bucket}/{key}",      get(get_object))
        .route("/{bucket}/{key}",      head(head_object))
        .route("/{bucket}/{key}",      delete(delete_object))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Cannot bind port");

    println!("  Listening...\n");
    axum::serve(listener, app).await.unwrap();
}

// ── PUT Object ────────────────────────────────────────────────────────────────

async fn put_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let data = body.to_vec();
    let size = data.len();
    println!("  [PUT] s3://{}/{} ({} bytes)", bucket, key, size);

    let etag = hex::encode(&blake3::hash(&data).as_bytes()[..16]);
    let safe_key = key.replace('/', "_").replace(' ', "_");
    let dna_path = format!("{}/{}_{}.dna", state.storage_dir, bucket, safe_key);

    match encode_to_dna(&data, &dna_path) {
        Ok(()) => {
            let ct = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();

            let meta = ObjectMeta {
                key: key.clone(),
                bucket: bucket.clone(),
                size,
                etag: etag.clone(),
                last_modified: Utc::now().to_rfc3339(),
                dna_file: PathBuf::from(&dna_path),
                content_type: ct,
            };

            let mut s = state.store.write().await;
            s.buckets.entry(bucket.clone())
                .or_insert_with(|| Utc::now().to_rfc3339());
            s.objects.entry(bucket).or_default().insert(key, meta);

            println!("  [PUT] -> {}", dna_path);

            let mut h = HeaderMap::new();
            h.insert("ETag", hv(&format!("\"{}\"", etag)));
            h.insert("x-amz-request-id", hv(&Uuid::new_v4().to_string()));
            h.insert("x-sovereign-encoded", HeaderValue::from_static("true"));
            (StatusCode::OK, h)
        }
        Err(e) => {
            eprintln!("  [PUT] error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, HeaderMap::new())
        }
    }
}

// ── GET Object ────────────────────────────────────────────────────────────────

async fn get_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    println!("  [GET] s3://{}/{}", bucket, key);
    let s = state.store.read().await;
    let meta = match s.objects.get(&bucket).and_then(|b| b.get(&key)) {
        Some(m) => m.clone(),
        None => return (StatusCode::NOT_FOUND, s3_err("NoSuchKey", &key)).into_response(),
    };
    drop(s);

    match decode_from_dna(&meta.dna_file, meta.size) {
        Ok(data) => {
            println!("  [GET] decoded {} bytes", data.len());
            let mut h = HeaderMap::new();
            h.insert(header::CONTENT_TYPE,    hv(&meta.content_type));
            h.insert(header::CONTENT_LENGTH,  hv(&data.len().to_string()));
            h.insert("ETag",                  hv(&format!("\"{}\"", meta.etag)));
            h.insert("Last-Modified",         hv(&meta.last_modified));
            h.insert("x-amz-request-id",      hv(&Uuid::new_v4().to_string()));
            (StatusCode::OK, h, data).into_response()
        }
        Err(e) => {
            eprintln!("  [GET] error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, s3_err("InternalError", &e)).into_response()
        }
    }
}

// ── HEAD Object ───────────────────────────────────────────────────────────────

async fn head_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    println!("  [HEAD] s3://{}/{}", bucket, key);
    let s = state.store.read().await;
    match s.objects.get(&bucket).and_then(|b| b.get(&key)) {
        Some(meta) => {
            let mut h = HeaderMap::new();
            h.insert(header::CONTENT_LENGTH, hv(&meta.size.to_string()));
            h.insert(header::CONTENT_TYPE,   hv(&meta.content_type));
            h.insert("ETag",                 hv(&format!("\"{}\"", meta.etag)));
            h.insert("Last-Modified",        hv(&meta.last_modified));
            (StatusCode::OK, h)
        }
        None => (StatusCode::NOT_FOUND, HeaderMap::new()),
    }
}

// ── DELETE Object ─────────────────────────────────────────────────────────────

async fn delete_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    println!("  [DELETE] s3://{}/{}", bucket, key);
    let mut s = state.store.write().await;
    if let Some(b) = s.objects.get_mut(&bucket) {
        if let Some(meta) = b.remove(&key) {
            let _ = std::fs::remove_file(&meta.dna_file);
        }
    }
    StatusCode::NO_CONTENT
}

// ── CREATE Bucket ─────────────────────────────────────────────────────────────

async fn create_bucket(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    println!("  [CREATE BUCKET] {}", bucket);
    let mut s = state.store.write().await;
    s.buckets.insert(bucket.clone(), Utc::now().to_rfc3339());
    s.objects.entry(bucket).or_default();
    StatusCode::OK
}

// ── LIST Objects ──────────────────────────────────────────────────────────────

async fn list_objects(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    println!("  [LIST] s3://{}", bucket);
    let s = state.store.read().await;
    let objects = s.objects.get(&bucket).cloned().unwrap_or_default();

    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n\
         <Name>{}</Name><KeyCount>{}</KeyCount>\
         <MaxKeys>1000</MaxKeys><IsTruncated>false</IsTruncated>\n",
        bucket, objects.len()
    );
    for (_, m) in &objects {
        xml.push_str(&format!(
            "<Contents><Key>{}</Key><Size>{}</Size>\
             <ETag>\"{}\"</ETag><LastModified>{}</LastModified>\
             <StorageClass>SOVEREIGN_DNA</StorageClass></Contents>\n",
            m.key, m.size, m.etag, m.last_modified
        ));
    }
    xml.push_str("</ListBucketResult>");

    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/xml"));
    (StatusCode::OK, h, xml)
}

// ── LIST Buckets ──────────────────────────────────────────────────────────────

async fn list_buckets(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.store.read().await;
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <ListAllMyBucketsResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\n\
         <Owner><ID>sovereign</ID><DisplayName>SovereignFlow</DisplayName></Owner>\n\
         <Buckets>\n"
    );
    for (name, created) in &s.buckets {
        xml.push_str(&format!(
            "<Bucket><Name>{}</Name><CreationDate>{}</CreationDate></Bucket>\n",
            name, created
        ));
    }
    xml.push_str("</Buckets>\n</ListAllMyBucketsResult>");
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/xml"));
    (StatusCode::OK, h, xml)
}

// ── DNA bridge ────────────────────────────────────────────────────────────────

fn encode_to_dna(data: &[u8], path: &str) -> Result<(), String> {
    use sovereign_vault::{raptor_encode, RaptorConfig};
    let (packets, oti) = raptor_encode(data, &RaptorConfig::default());
    let mut lines = vec![format!("SOVEREIGN_VAULT_V1:{},{},{}",
        data.len(), oti.transfer_length(), oti.symbol_size())];
    for (i, p) in packets.iter().enumerate() {
        let b = p.serialize();
        lines.push(format!("{}:{}:{}:{}", i, blake3_short(&b), hex_enc(&b), atgc(&b)));
    }
    std::fs::write(path, lines.join("\n")).map_err(|e| e.to_string())
}

fn decode_from_dna(path: &PathBuf, original_len: usize) -> Result<Vec<u8>, String> {
    use sovereign_vault::raptor_decode;
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut lines = content.lines();
    let hdr = lines.next().ok_or("missing header")?;
    let csv = hdr.strip_prefix("SOVEREIGN_VAULT_V1:").ok_or("bad header")?;
    let p: Vec<&str> = csv.split(',').collect();
    let tl: u64 = p[1].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    let ss: u16 = p[2].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    let oti = raptorq::ObjectTransmissionInformation::with_defaults(tl, ss);
    let packets: Vec<Option<raptorq::EncodingPacket>> = lines
        .filter_map(|l| {
            let p: Vec<&str> = l.splitn(4, ':').collect();
            if p.len() == 4 {
                Some(Some(raptorq::EncodingPacket::deserialize(&hex_dec(p[2]))))
            } else { None }
        })
        .collect();
    match raptor_decode(&packets, oti) {
        Some(mut d) => { d.truncate(original_len); Ok(d) }
        None => Err("decode failed".to_string()),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn hv(s: &str) -> HeaderValue { HeaderValue::from_str(s).unwrap() }

fn s3_err(code: &str, msg: &str) -> String {
    format!("<?xml version=\"1.0\"?><Error><Code>{}</Code><Message>{}</Message></Error>",
        code, msg)
}

fn atgc(data: &[u8]) -> String {
    const B: [char; 4] = ['A','T','G','C'];
    data.iter().flat_map(|b| [
        B[((b>>6)&3) as usize], B[((b>>4)&3) as usize],
        B[((b>>2)&3) as usize], B[(b&3) as usize],
    ]).collect()
}

fn blake3_short(data: &[u8]) -> String {
    let h = blake3::hash(data);
    format!("{:02x}{:02x}{:02x}{:02x}",
        h.as_bytes()[0], h.as_bytes()[1], h.as_bytes()[2], h.as_bytes()[3])
}

fn hex_enc(b: &[u8]) -> String { b.iter().map(|x| format!("{:02x}", x)).collect() }

fn hex_dec(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i+2], 16).unwrap_or(0))
        .collect()
}
