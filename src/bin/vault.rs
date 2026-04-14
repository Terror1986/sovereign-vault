use std::{env, fs, process};
use sovereign_vault::{
    raptor_encode, raptor_decode,
    RaptorConfig,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: vault <encode|decode> <infile> <outfile>");
        process::exit(1);
    }
    match args[1].as_str() {
        "encode" => encode(&args[2], &args[3]),
        "decode" => decode(&args[2], &args[3]),
        _ => { eprintln!("Unknown command: {}", args[1]); process::exit(1); }
    }
}

fn encode(infile: &str, outfile: &str) {
    let data = fs::read(infile).expect("Cannot read input file");
    println!("[encode] {} bytes from {}", data.len(), infile);

    let (packets, oti) = raptor_encode(&data, &RaptorConfig::default());

    let tl = oti.transfer_length();
    let ss = oti.symbol_size();

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("SOVEREIGN_VAULT_V1:{},{},{}", data.len(), tl, ss));

    // Store each packet as hex — simple, correct, no reconstruction ambiguity
    for (i, packet) in packets.iter().enumerate() {
        let bytes = packet.serialize();
        let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        // Also encode to ATGC for the biological layer (informational)
        let atgc = bytes_to_atgc(&bytes);
        let hash = blake3_short(&bytes);
        lines.push(format!("{}:{}:{}:{}", i, hash, hex, atgc));
    }

    fs::write(outfile, lines.join("\n")).expect("Cannot write output file");
    let out_size = fs::metadata(outfile).unwrap().len() as usize;
    println!("[encode] {} packets -> {}", packets.len(), outfile);
    println!("[encode] {} bytes -> {} bytes ({:.1}x expansion)",
        data.len(), out_size, out_size as f64 / data.len() as f64);
    println!("[encode] done ✅");
}

fn decode(infile: &str, outfile: &str) {
    let content = fs::read_to_string(infile).expect("Cannot read .dna file");
    let mut lines = content.lines();

    let header = lines.next().expect("Missing header");
    let csv = header.strip_prefix("SOVEREIGN_VAULT_V1:").expect("Bad header");
    let parts: Vec<&str> = csv.split(',').collect();
    let original_len: usize = parts[0].parse().unwrap();
    let transfer_length: u64 = parts[1].parse().unwrap();
    let symbol_size: u16 = parts[2].parse().unwrap();
    let oti = raptorq::ObjectTransmissionInformation::with_defaults(transfer_length, symbol_size);
    println!("[decode] original length: {} bytes", original_len);

    let mut recovered_packets: Vec<Option<raptorq::EncodingPacket>> = Vec::new();

    for line in lines {
        let p: Vec<&str> = line.splitn(4, ':').collect();
        if p.len() != 4 { continue; }
        let hex = p[2];
        let bytes = hex_decode(hex);
        let pkt = raptorq::EncodingPacket::deserialize(&bytes);
        recovered_packets.push(Some(pkt));
    }

    let usable = recovered_packets.iter().filter(|p| p.is_some()).count();
    println!("[decode] {}/{} packets, running RaptorQ...", usable, recovered_packets.len());

    match raptor_decode(&recovered_packets, oti) {
        Some(mut data) => {
            data.truncate(original_len);
            fs::write(outfile, &data).expect("Cannot write output");
            println!("[decode] {} bytes -> {} ✅", data.len(), outfile);
        }
        None => {
            eprintln!("[decode] FAILED — insufficient packets");
            process::exit(1);
        }
    }
}

fn bytes_to_atgc(data: &[u8]) -> String {
    const BASES: [char; 4] = ['A', 'T', 'G', 'C'];
    let mut out = String::new();
    for byte in data {
        out.push(BASES[((byte >> 6) & 3) as usize]);
        out.push(BASES[((byte >> 4) & 3) as usize]);
        out.push(BASES[((byte >> 2) & 3) as usize]);
        out.push(BASES[(byte & 3) as usize]);
    }
    out
}

fn blake3_short(data: &[u8]) -> String {
    let h = blake3::hash(data);
    format!("{:02x}{:02x}{:02x}{:02x}",
        h.as_bytes()[0], h.as_bytes()[1],
        h.as_bytes()[2], h.as_bytes()[3])
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i+2], 16).unwrap_or(0))
        .collect()
}
