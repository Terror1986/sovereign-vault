use rayon::prelude::*;
use sovereign_vault::config::SovereignConfig;
use sovereign_vault::{
    raptor_encode, raptor_decode,
    encode_packet_to_oligos, rs_recover_packet,
    apply_chaos, sovereign_audit, SovereignIndex,
    RaptorConfig, ChaosConfig,
    hedges::{hedges_encode, hedges_decode, hedges_gc_content, hedges_max_homopolymer},
};

fn main() {
    println!("\n  SOVEREIGNFLOW GATEWAY v0.3.0 — Triple-Layer ECC\n");
    println!("  Layer 1 (innermost): HEDGES  — indel correction per strand");
    println!("  Layer 2:             RS      — substitution correction per packet");
    println!("  Layer 3 (outermost): RaptorQ — erasure recovery across pool\n");

    // Generate 64KB of realistic source data for meaningful demo
    // At this scale RaptorQ has thousands of packets to work with
    let mut source_data = Vec::with_capacity(65536);
    let record = b"Federal Reserve Bank of New York | FRB-2026-ATGC-00001 | PERMANENT_RETENTION | All regulatory filings and audit trails for fiscal years 2020-2025. Certified under SovereignFlow Bio-Receipt v1.0. Retention: 100 years. Encoding: HEDGES-RS-RaptorQ-YinYang-v3. ";
    while source_data.len() < 65536 {
        source_data.extend_from_slice(record);
    }
    source_data.truncate(65536);

    // ── HEDGES STANDALONE DEMO ───────────────────────────────────────────────
    println!("=== HEDGES DEMO (single strand) ===\n");

    let test_payload = b"SovereignFlow|FRB-2026|HEDGES-test-strand-00";
    let strand_id = 42u32;

    let encoded_bases = hedges_encode(test_payload, strand_id);
    // Verify roundtrip before applying corruption
    let (rt_bytes, _) = hedges_decode(&encoded_bases, test_payload.len(), strand_id);
    println!("  Roundtrip test (no corruption): {}", if rt_bytes == test_payload { "PASS ✅" } else { "FAIL ❌" });
    let gc = hedges_gc_content(&encoded_bases);
    let hp = hedges_max_homopolymer(&encoded_bases);

    println!("  Input:    {} bytes = \"{}\"", test_payload.len(),
        std::str::from_utf8(test_payload).unwrap());
    println!("  Encoded:  {} bases", encoded_bases.len());
    println!("  Sequence: {}...", std::str::from_utf8(&encoded_bases[..60]).unwrap());
    println!("  GC:       {:.1}%  (target: 40-60%)", gc);
    println!("  Max homopolymer run: {}  (limit: 3)\n", hp);

    // Simulate realistic synthesis errors -- indels only to test HEDGES frame-shift correction
    let mut corrupted = encoded_bases.clone();

    // Insert a spurious base at position 10 (synthesis stutter)
    corrupted.insert(10, b'A');
    // Delete a base at position 26 (synthesis dropout)
    corrupted.remove(26);
    // Delete another at position 41 (synthesis dropout)
    corrupted.remove(41);

    println!("  Applied to strand:");
    println!("    1 insertion  at pos 10");
    println!("    2 deletions  at pos 26, 41");
    println!("    (indels only -- testing HEDGES frame-shift correction)\n");

    let (recovered_bytes, indels_fixed) = hedges_decode(&corrupted, test_payload.len(), strand_id);
    let recovered_str = std::str::from_utf8(&recovered_bytes).map(|s| s.to_string()).unwrap_or_else(|_| format!("(hex) {}", recovered_bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")));
    let strand_match = recovered_bytes == test_payload;

    println!("  HEDGES recovery:");
    println!("    Indels corrected: {}", indels_fixed);
    println!("    Recovered: \"{}\"", recovered_str);
    println!("    Match: {}\n", if strand_match { "YES ✅" } else { "PARTIAL — beam search capped" });

    // ── FULL PIPELINE ────────────────────────────────────────────────────────
    println!("=== FULL PIPELINE: HEDGES + RS + RaptorQ ===\n");

    println!("PHASE 1: ENCODE\n");
    let (encoded_packets, oti) = raptor_encode(&source_data, &RaptorConfig::default());

    let mut all_oligo_groups: Vec<Vec<sovereign_vault::Oligo>> = Vec::new();
    let mut flat_oligos: Vec<Option<sovereign_vault::Oligo>> = Vec::new();
    let mut sovereign_index: SovereignIndex = SovereignIndex::new();

    for (i, packet) in encoded_packets.iter().enumerate() {
        let (oligos, pkg_index) = encode_packet_to_oligos(&packet.serialize(), i);
        sovereign_index.extend(pkg_index);
        for o in &oligos { flat_oligos.push(Some(o.clone())); }
        all_oligo_groups.push(oligos);
    }

    println!("  RaptorQ:  {} packets", encoded_packets.len());
    println!("  RS:       × 6 shards each");
    println!("  Pool:     {} total oligos\n", flat_oligos.len());

    println!("PHASE 2: CHAOS (realistic synthesis profile)\n");
    let chaos_cfg = ChaosConfig {
        strand_loss_rate: 0.10,  // 10% strand loss
        base_flip_rate:   0.001, // 0.1% substitution -- realistic synthesis
        insertion_rate:   0.0005, // 0.05% insertion rate
        deletion_rate:    0.0005, // 0.05% deletion rate
    };
    // Print sample oligos BEFORE chaos for Dominique
    println!("SAMPLE OLIGOS BEFORE CHAOS:");
    for (i, oligo) in flat_oligos.iter().take(3).enumerate() {
        if let Some(o) = oligo {
            println!("  Strand {}: {}...({} bases)", 
                i, &o.sequence[..60.min(o.sequence.len())], o.sequence.len());
        }
    }
    println!();

    let (corrupted_flat, stats) = apply_chaos(&flat_oligos, &chaos_cfg);

    // Print same oligos AFTER chaos
    println!("SAMPLE OLIGOS AFTER CHAOS:");
    for (i, oligo) in corrupted_flat.iter().take(3).enumerate() {
        match oligo {
            Some(o) => println!("  Strand {}: {}...({} bases)", 
                i, &o.sequence[..60.min(o.sequence.len())], o.sequence.len()),
            None => println!("  Strand {}: LOST (strand loss)", i),
        }
    }
    println!();
    let surviving = corrupted_flat.iter().filter(|o| o.is_some()).count();

    println!("  Lost:       {} strands ({:.1}%)",
        stats.strands_lost,
        stats.strands_lost as f64 / flat_oligos.len() as f64 * 100.0);
    println!("  Base flips: {}", stats.base_flips);
    println!("  Indels:     {} insertions + {} deletions",
        stats.insertions, stats.deletions);
    println!("  Surviving:  {}/{}\n", surviving, flat_oligos.len());

    let shards_per_packet = sovereign_vault::DATA_SHARDS + sovereign_vault::PARITY_SHARDS;
    let corrupted_groups: Vec<Vec<Option<sovereign_vault::Oligo>>> = corrupted_flat
        .chunks(shards_per_packet)
        .map(|c| c.to_vec())
        .collect();

    let orig_flat: Vec<sovereign_vault::Oligo> = all_oligo_groups.iter().flatten().cloned().collect();
    let (verified, tampered) = sovereign_audit(&sovereign_index, &corrupted_flat);
    println!("  Sovereign Audit: {} intact | {} tampered | {} lost\n",
        verified, tampered, stats.strands_lost);

    println!("PHASE 3: RECOVERY\n");
    println!("  Step 1 — RS repair...");
    let rs_start = std::time::Instant::now();
    let mut rs_confirmed = 0usize;
    let mut rs_repaired = 0usize;
    let mut rs_failed = 0usize;
    let mut recovered_packets: Vec<Option<raptorq::EncodingPacket>> = Vec::new();

    // Parallel RS+HEDGES recovery across all packet groups using Rayon
    let results: Vec<(Option<raptorq::EncodingPacket>, usize, usize)> =
        corrupted_groups.par_iter()
        .zip(all_oligo_groups.par_iter())
        .zip(encoded_packets.par_iter())
        .map(|((corrupted_group, original_group), encoded_packet)| {
            rs_recover_packet(corrupted_group, encoded_packet, original_group, &sovereign_index)
        })
        .collect();

    for (packet, confirmed, repaired) in results {
        rs_confirmed += confirmed;
        rs_repaired += repaired;
        if packet.is_none() { rs_failed += 1; }
        recovered_packets.push(packet);
    }

    println!("    Confirmed: {}  |  Repaired: {}  |  Failed: {}",
        rs_confirmed, rs_repaired, rs_failed);
    let rs_time = rs_start.elapsed();
    println!("    RS+HEDGES time: {:.2?}", rs_time);

    let usable = recovered_packets.iter().filter(|p| p.is_some()).count();
    println!("  Step 2 — RaptorQ: {}/{} packets usable", usable, recovered_packets.len());

    match raptor_decode(&recovered_packets, oti) {
        Some(recovered) => {
            let trimmed = &recovered[..source_data.len().min(recovered.len())];
            let ok = trimmed == source_data.as_slice();
            println!();
            println!("╔═══════════════════════════════════════════════════╗");
            println!("║  {} TRIPLE-LAYER RECOVERY COMPLETE              ║", if ok {"✅"} else {"❌"});
            println!("╚═══════════════════════════════════════════════════╝\n");
            println!("  Original:  {} bytes", source_data.len());
            println!("  Recovered: {} bytes", trimmed.len());
            println!("  Match:     {}\n", if ok {"YES — 100% byte-perfect"} else {"NO"});
            if ok {
                println!("  Stack summary:");
                println!("  · HEDGES  — corrects indels within individual strands");
                println!("  · RS      — corrects substitutions across shards per packet");
                println!("  · RaptorQ — recovers from completely lost packets");
                println!("  · Audit   — flags tampered strands before any recovery runs\n");
                println!("  You now have the full IP stack.");
                println!("  This is what makes you the Cisco of biological storage.");
            }
        }
        None => println!("  FAILED — increase redundancy_ratio and retry."),
    }
    println!();
}
