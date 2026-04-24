use sovereign_vault::gpu_accel::GpuAccelerator;
use std::time::Instant;

fn main() {
    println!("\n  SOVEREIGNFLOW GPU BATCH BENCHMARK");
    println!("  ===================================\n");

    let gpu = GpuAccelerator::new().expect("GPU init failed");
    println!("GPU: {}\n", gpu.device_name());

    // Simulate RaptorQ workload -- many symbols XORed together
    let symbol_size = 1024; // typical RaptorQ symbol size
    let batch_sizes = vec![100, 1000, 10000, 100000];

    for num_symbols in batch_sizes {
        let total_bytes = symbol_size * num_symbols;
        let a = vec![0xAA_u8; total_bytes];
        let b = vec![0x55_u8; total_bytes];

        // CPU baseline -- process all symbols
        let cpu_start = Instant::now();
        let _cpu_result: Vec<u8> = a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect();
        let cpu_time = cpu_start.elapsed();
        let cpu_throughput = (total_bytes as f64 / cpu_time.as_secs_f64()) / 1e9;

        // GPU -- single transfer, process all at once
        let gpu_start = Instant::now();
        let _gpu_result = gpu.xor_symbols(&a, &b).expect("GPU XOR failed");
        let gpu_time = gpu_start.elapsed();
        let gpu_throughput = (total_bytes as f64 / gpu_time.as_secs_f64()) / 1e9;

        let speedup = gpu_throughput / cpu_throughput;

        println!("Symbols: {:>6} | Total: {:>8} KB | CPU: {:>6.2} GB/s | GPU: {:>6.2} GB/s | Speedup: {:>5.1}x",
            num_symbols,
            total_bytes / 1024,
            cpu_throughput,
            gpu_throughput,
            speedup);
    }

    println!("\nRTX 3080 Ti batch XOR benchmark complete");
}
