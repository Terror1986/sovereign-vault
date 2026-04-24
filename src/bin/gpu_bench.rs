use sovereign_vault::gpu_accel::GpuAccelerator;

fn main() {
    println!("\n  SOVEREIGNFLOW PURE GPU COMPUTE BENCHMARK");
    println!("  ==========================================\n");
    println!("  Testing XOR throughput with data pre-loaded in VRAM");
    println!("  (No transfer overhead -- pure GPU compute speed)\n");

    let gpu = GpuAccelerator::new().expect("GPU init failed");
    println!("GPU: {}", gpu.device_name());
    gpu.warmup();
    println!();

    let sizes = vec![
        (1024 * 1024,      "1 MB"),
        (10 * 1024 * 1024, "10 MB"),
        (100 * 1024 * 1024,"100 MB"),
        (500 * 1024 * 1024,"500 MB"),
    ];

    println!("{:<12} {:<16} {:<12}", "Buffer Size", "GPU Throughput", "vs CPU");
    println!("{}", "-".repeat(40));

    let cpu_baseline = 2.0; // GB/s typical CPU XOR

    for (size, label) in sizes {
        match gpu.benchmark_vram_xor(size) {
            Ok(throughput) => {
                let speedup = throughput / cpu_baseline;
                println!("{:<12} {:<16} {:.1}x faster",
                    label,
                    format!("{:.2} GB/s", throughput),
                    speedup);
            }
            Err(e) => println!("{:<12} Error: {}", label, e),
        }
    }

    println!("\nThis is the true GPU XOR speed when data lives in VRAM.");
    println!("Full pipeline: upload strand pool once, repair entirely in VRAM.");
}
