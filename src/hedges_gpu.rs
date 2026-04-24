//! GPU HEDGES Beam Search
//! 
//! Parallelizes HEDGES across strands -- each strand decoded simultaneously.
//! Architecture: one CUDA thread block per strand, 128 threads per block.
//! 7,992 strands * 128 threads = ~1M threads running simultaneously.

use cudarc::driver::{CudaDevice, LaunchAsync, LaunchConfig};
use cudarc::nvrtc::compile_ptx;
use std::sync::Arc;

const HEDGES_KERNEL: &str = include_str!("hedges_gpu.cu");

pub struct HedgesGpu {
    device: Arc<CudaDevice>,
}

impl HedgesGpu {
    pub fn new() -> Result<Self, String> {
        let device = CudaDevice::new(0)
            .map_err(|e| format!("CUDA init failed: {}", e))?;

        let ptx = compile_ptx(HEDGES_KERNEL)
            .map_err(|e| format!("HEDGES PTX compile failed: {}", e))?;

        device.load_ptx(ptx, "hedges", &["hedges_decode_batch"])
            .map_err(|e| format!("HEDGES PTX load failed: {}", e))?;

        Ok(HedgesGpu { device })
    }

    /// Decode multiple strands simultaneously on GPU
    /// Returns (decoded_bytes_per_strand, indel_counts)
    pub fn decode_strands(
        &self,
        strands: &[Vec<u8>],
        expected_bytes: usize,
        strand_ids: &[u32],
    ) -> Result<(Vec<Vec<u8>>, Vec<usize>), String> {
        let num_strands = strands.len();
        let max_bases = 200usize;

        // Pack strands into flat array
        let mut packed_strands = vec![b'N'; num_strands * max_bases];
        let mut strand_lengths = vec![0i32; num_strands];

        for (i, strand) in strands.iter().enumerate() {
            let len = strand.len().min(max_bases);
            packed_strands[i * max_bases..i * max_bases + len].copy_from_slice(&strand[..len]);
            strand_lengths[i] = len as i32;
        }

        let strand_ids_i32: Vec<i32> = strand_ids.iter().map(|&x| x as i32).collect();

        // Upload to GPU
        let gpu_strands = self.device.htod_sync_copy(&packed_strands)
            .map_err(|e| format!("Upload strands failed: {}", e))?;
        let gpu_lengths = self.device.htod_sync_copy(&strand_lengths)
            .map_err(|e| format!("Upload lengths failed: {}", e))?;
        let gpu_ids = self.device.htod_sync_copy(&strand_ids_i32)
            .map_err(|e| format!("Upload IDs failed: {}", e))?;

        let mut gpu_output = self.device.alloc_zeros::<u8>(num_strands * expected_bytes)
            .map_err(|e| format!("Alloc output failed: {}", e))?;
        let mut gpu_indels = self.device.alloc_zeros::<i32>(num_strands)
            .map_err(|e| format!("Alloc indels failed: {}", e))?;

        // Launch -- one block per strand, 128 threads per block
        let cfg = LaunchConfig {
            grid_dim: (num_strands as u32, 1, 1),
            block_dim: (128, 1, 1),
            shared_mem_bytes: 0,
        };

        let kernel = self.device.get_func("hedges", "hedges_decode_batch")
            .ok_or_else(|| "HEDGES kernel not found".to_string())?;

        unsafe {
            kernel.launch(cfg, (
                &gpu_strands,
                &gpu_lengths,
                &gpu_ids,
                expected_bytes as i32,
                &mut gpu_output,
                &mut gpu_indels,
                num_strands as i32,
            )).map_err(|e| format!("HEDGES launch failed: {}", e))?;
        }

        self.device.synchronize()
            .map_err(|e| format!("Sync failed: {}", e))?;

        // Download results
        let flat_output = self.device.dtoh_sync_copy(&gpu_output)
            .map_err(|e| format!("Download output failed: {}", e))?;
        let flat_indels = self.device.dtoh_sync_copy(&gpu_indels)
            .map_err(|e| format!("Download indels failed: {}", e))?;

        // Unpack
        let decoded: Vec<Vec<u8>> = (0..num_strands)
            .map(|i| flat_output[i * expected_bytes..(i + 1) * expected_bytes].to_vec())
            .collect();
        let indels: Vec<usize> = flat_indels.iter().map(|&x| x as usize).collect();

        Ok((decoded, indels))
    }

    pub fn device_name(&self) -> String {
        self.device.name().unwrap_or_else(|_| "Unknown".to_string())
    }
}
