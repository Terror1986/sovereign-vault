//! GPU Acceleration for SovereignFlow
//! 
//! Offloads RaptorQ XOR operations to CUDA for massive parallelism.
//! RTX 3080 Ti: 10,496 CUDA cores vs 4 CPU cores = ~2,600x theoretical parallelism.
//! Target: 10+ Gb/s decode throughput.

use cudarc::driver::{CudaDevice, CudaSlice, LaunchAsync, LaunchConfig};
use cudarc::nvrtc::compile_ptx;
use std::sync::Arc;

const XOR_KERNEL: &str = r#"
extern "C" __global__ void xor_symbols(
    const unsigned char* a,
    const unsigned char* b, 
    unsigned char* out,
    int len
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < len) {
        out[idx] = a[idx] ^ b[idx];
    }
}

// Batch XOR -- process multiple symbol pairs in one kernel launch
// symbols: flat array of [sym0_a, sym0_b, sym1_a, sym1_b, ...]
// out: flat array of results
extern "C" __global__ void xor_batch(
    const unsigned char* symbols_a,
    const unsigned char* symbols_b,
    unsigned char* out,
    int symbol_len,
    int num_symbols
) {
    int total = symbol_len * num_symbols;
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < total) {
        out[idx] = symbols_a[idx] ^ symbols_b[idx];
    }
}
"#;

pub struct GpuAccelerator {
    device: Arc<CudaDevice>,
}

impl GpuAccelerator {
    pub fn new() -> Result<Self, String> {
        let device = CudaDevice::new(0)
            .map_err(|e| format!("Failed to initialize CUDA device: {}", e))?;
        Ok(GpuAccelerator { device })
    }

    /// XOR two symbol arrays on GPU -- core RaptorQ operation
    pub fn xor_symbols(&self, a: &[u8], b: &[u8]) -> Result<Vec<u8>, String> {
        assert_eq!(a.len(), b.len(), "Symbol lengths must match");
        let len = a.len();

        // Compile kernel
        let ptx = compile_ptx(XOR_KERNEL)
            .map_err(|e| format!("PTX compile failed: {}", e))?;
        
        self.device.load_ptx(ptx, "xor", &["xor_symbols"])
            .map_err(|e| format!("PTX load failed: {}", e))?;

        // Copy data to GPU
        let gpu_a = self.device.htod_sync_copy(a)
            .map_err(|e| format!("GPU copy failed: {}", e))?;
        let gpu_b = self.device.htod_sync_copy(b)
            .map_err(|e| format!("GPU copy failed: {}", e))?;
        let mut gpu_out: CudaSlice<u8> = self.device.alloc_zeros(len)
            .map_err(|e| format!("GPU alloc failed: {}", e))?;

        // Launch kernel
        let threads = 256u32;
        let blocks = ((len as u32) + threads - 1) / threads;
        let cfg = LaunchConfig {
            block_dim: (threads, 1, 1),
            grid_dim: (blocks, 1, 1),
            shared_mem_bytes: 0,
        };

        let kernel = self.device.get_func("xor", "xor_symbols")
            .ok_or_else(|| "Get kernel failed: function not found".to_string())?;

        unsafe {
            kernel.launch(cfg, (&gpu_a, &gpu_b, &mut gpu_out, len as i32))
                .map_err(|e| format!("Kernel launch failed: {}", e))?;
        }

        // Copy result back
        let result = self.device.dtoh_sync_copy(&gpu_out)
            .map_err(|e| format!("GPU result copy failed: {}", e))?;

        Ok(result)
    }

    pub fn device_name(&self) -> String {
        self.device.name().unwrap_or_else(|_| "Unknown GPU".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_xor() {
        let gpu = GpuAccelerator::new().expect("GPU init failed");
        println!("GPU: {}", gpu.device_name());
        
        let a = vec![0xAA_u8; 1024];
        let b = vec![0x55_u8; 1024];
        let result = gpu.xor_symbols(&a, &b).expect("XOR failed");
        
        // 0xAA ^ 0x55 = 0xFF
        assert!(result.iter().all(|&x| x == 0xFF));
        println!("GPU XOR test: PASS");
    }
}
