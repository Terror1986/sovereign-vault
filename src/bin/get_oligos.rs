use sovereign_vault::hedges::hedges_encode;

fn main() {
    // 17 bytes = 136 bases -- exact Atlas target
    let data = vec![0xAB_u8; 17];
    for strand_id in 0..5u32 {
        let encoded = hedges_encode(&data, strand_id);
        let seq = String::from_utf8(encoded).unwrap();
        println!("{}", seq);
    }
}
