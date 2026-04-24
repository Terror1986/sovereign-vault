
// HEDGES Beam Search CUDA Kernel
// Each thread block processes one DNA strand independently
// Threads within a block collaborate on the 128-hypothesis beam

#define BEAM_WIDTH 128
#define MAX_BITS 1024
#define MAX_BASES 200
#define MAX_INDEL_DEPTH 8

typedef struct {
    unsigned char bits[MAX_BITS / 8]; // packed bits
    unsigned long long state;
    int seq_pos;
    int bit_pos;
    unsigned char prev_base;
    float score;
    int indel_corrections;
    int valid;
} Hypothesis;

__device__ unsigned long long update_state(unsigned long long state, int coded_bit, int bit_pos) {
    unsigned long long mixed = state ^ ((unsigned long long)coded_bit << (bit_pos & 63));
    // FNV-like mixing
    mixed ^= mixed >> 33;
    mixed *= 0xff51afd7ed558ccdULL;
    mixed ^= mixed >> 33;
    mixed *= 0xc4ceb9fe1a85ec53ULL;
    mixed ^= mixed >> 33;
    return mixed;
}

__device__ unsigned char hedges_pad(unsigned long long state, int bit_pos) {
    unsigned long long h = state ^ ((unsigned long long)bit_pos * 0x9e3779b97f4a7c15ULL);
    h ^= h >> 30;
    h *= 0xbf58476d1ce4e5b9ULL;
    h ^= h >> 27;
    h *= 0x94d049bb133111ebULL;
    h ^= h >> 31;
    return (unsigned char)(h & 1);
}

__device__ int base_to_bit(unsigned char base, int bit_pos, unsigned char prev_base) {
    // Standard HEDGES base encoding
    if (base == 'A') return 0;
    if (base == 'T') return 1;
    if (base == 'G') return 0;
    if (base == 'C') return 1;
    return -1; // invalid
}

// One thread block per strand, one thread per hypothesis
extern "C" __global__ void hedges_decode_batch(
    const unsigned char* strands,      // input: all strands packed
    const int* strand_lengths,         // length of each strand
    const int* strand_ids,             // strand IDs for hash init
    const int expected_bytes,          // expected output bytes per strand
    unsigned char* output,             // output: decoded bytes
    int* indel_counts,                 // output: indels corrected per strand
    int num_strands
) {
    int strand_idx = blockIdx.x;
    int thread_idx = threadIdx.x; // one thread per hypothesis slot

    if (strand_idx >= num_strands) return;
    if (thread_idx >= BEAM_WIDTH) return;

    // Shared memory for beam hypotheses
    __shared__ Hypothesis beam[BEAM_WIDTH];
    __shared__ Hypothesis next_beam[BEAM_WIDTH];
    __shared__ int beam_size;
    __shared__ int found_winner;
    __shared__ int winner_idx;

    const unsigned char* strand = strands + strand_idx * MAX_BASES;
    int strand_len = strand_lengths[strand_idx];
    int strand_id = strand_ids[strand_idx];
    int expected_bits = expected_bytes * 8;

    // Initialize -- thread 0 sets up first hypothesis
    if (thread_idx == 0) {
        beam_size = 1;
        found_winner = 0;
        winner_idx = -1;

        // Initial state from strand_id hash
        unsigned long long init_state = (unsigned long long)strand_id * 0x9e3779b97f4a7c15ULL;
        init_state ^= init_state >> 33;
        init_state *= 0xff51afd7ed558ccdULL;

        beam[0].state = init_state;
        beam[0].seq_pos = 0;
        beam[0].bit_pos = 0;
        beam[0].prev_base = 'N';
        beam[0].score = 0.0f;
        beam[0].indel_corrections = 0;
        beam[0].valid = 1;
        for (int i = 0; i < MAX_BITS/8; i++) beam[0].bits[i] = 0;
    }

    __syncthreads();

    // Beam search iterations
    for (int iter = 0; iter < expected_bits + MAX_INDEL_DEPTH * 2 && !found_winner; iter++) {
        if (thread_idx == 0) {
            // Check for winner
            for (int i = 0; i < beam_size; i++) {
                if (beam[i].valid && beam[i].bit_pos >= expected_bits) {
                    found_winner = 1;
                    winner_idx = i;
                    break;
                }
            }
        }
        __syncthreads();
        if (found_winner) break;

        // Each thread expands one hypothesis
        if (thread_idx < beam_size && beam[thread_idx].valid) {
            Hypothesis* h = &beam[thread_idx];
            // Normal decode
            if (h->seq_pos < strand_len) {
                unsigned char base = strand[h->seq_pos];
                unsigned char pad = hedges_pad(h->state, h->bit_pos);
                int coded_bit = base_to_bit(base, h->bit_pos, h->prev_base);
                if (coded_bit >= 0) {
                    int msg_bit = coded_bit ^ pad;
                    // Store bit
                    int byte_idx = h->bit_pos / 8;
                    int bit_idx = h->bit_pos % 8;
                    if (byte_idx < MAX_BITS/8) {
                        unsigned char new_bits = h->bits[byte_idx];
                        if (msg_bit) new_bits |= (1 << bit_idx);
                        else new_bits &= ~(1 << bit_idx);
                        next_beam[thread_idx] = *h;
                        next_beam[thread_idx].bits[byte_idx] = new_bits;
                        next_beam[thread_idx].state = update_state(h->state, coded_bit, h->bit_pos);
                        next_beam[thread_idx].seq_pos = h->seq_pos + 1;
                        next_beam[thread_idx].bit_pos = h->bit_pos + 1;
                        next_beam[thread_idx].prev_base = base;
                    }
                }
            }
        }

        __syncthreads();

        // Swap beams
        if (thread_idx < BEAM_WIDTH) {
            beam[thread_idx] = next_beam[thread_idx];
        }
        __syncthreads();
    }

    // Write output
    if (thread_idx == 0 && found_winner && winner_idx >= 0) {
        unsigned char* out = output + strand_idx * expected_bytes;
        for (int i = 0; i < expected_bytes; i++) {
            out[i] = beam[winner_idx].bits[i];
        }
        indel_counts[strand_idx] = beam[winner_idx].indel_corrections;
    }
}
