//! O(1) Circular Bitmask Deduplicator for GameTunnel
//!
//! Maintains a 1024-packet sliding window represented as a compact bitmap
//! of 16 x u64 integers (128 bytes total, cache-line friendly).
//!
//! Operates with ZERO heap allocations on the critical path.
//! Handles out-of-order arrivals, immediate duplicate drops, and u32 wrapping.

pub const WINDOW_SIZE: usize = 1024;
const WORDS: usize = WINDOW_SIZE / 64; // 16

#[derive(Debug, Clone)]
pub struct Deduplicator {
    highest_seq: Option<u32>,
    bitmap: [u64; WORDS],
    // Telemetry counters
    pub total_received: u64,
    pub accepted_in_order: u64,
    pub accepted_out_of_order: u64,
    pub duplicates_dropped: u64,
    pub stale_dropped: u64,
}

impl Default for Deduplicator {
    fn default() -> Self {
        Self::new()
    }
}

impl Deduplicator {
    #[inline]
    pub fn new() -> Self {
        Self {
            highest_seq: None,
            bitmap: [0u64; WORDS],
            total_received: 0,
            accepted_in_order: 0,
            accepted_out_of_order: 0,
            duplicates_dropped: 0,
            stale_dropped: 0,
        }
    }

    /// Reset the deduplicator state
    pub fn reset(&mut self) {
        self.highest_seq = None;
        self.bitmap.fill(0);
        self.total_received = 0;
        self.accepted_in_order = 0;
        self.accepted_out_of_order = 0;
        self.duplicates_dropped = 0;
        self.stale_dropped = 0;
    }

    /// Check if a packet sequence number is accepted (new and unique) or should be dropped.
    ///
    /// Returns:
    /// - `true`: Packet is fresh (either newest or valid out-of-order in window). Transmit it.
    /// - `false`: Packet is duplicate or too stale. Drop it immediately.
    #[inline]
    pub fn process_packet(&mut self, seq: u32) -> bool {
        self.total_received = self.total_received.wrapping_add(1);

        let highest = match self.highest_seq {
            None => {
                // First packet seen
                self.highest_seq = Some(seq);
                self.bitmap[0] = 1;
                self.accepted_in_order = self.accepted_in_order.wrapping_add(1);
                return true;
            }
            Some(h) => h,
        };

        let diff = seq.wrapping_sub(highest) as i32;

        if diff > 0 {
            // Newer packet arrived
            let shift = seq.wrapping_sub(highest) as usize;
            self.shift_window(shift);
            self.highest_seq = Some(seq);
            self.bitmap[0] |= 1;
            self.accepted_in_order = self.accepted_in_order.wrapping_add(1);
            true
        } else {
            // Same or older packet arrived
            let lag = highest.wrapping_sub(seq) as usize;
            if lag >= WINDOW_SIZE {
                if lag > 2048 {
                    // Sequence stream reset or client restarted with fresh sequence numbers.
                    // Resynchronize window to avoid permanently dropping all future packets.
                    self.reset();
                    self.highest_seq = Some(seq);
                    self.bitmap[0] = 1;
                    self.accepted_in_order = self.accepted_in_order.wrapping_add(1);
                    return true;
                }
                // Older than the 1024-packet window -> Stale
                self.stale_dropped = self.stale_dropped.wrapping_add(1);
                false
            } else {
                let word_idx = lag / 64;
                let bit_idx = lag % 64;
                let mask = 1u64 << bit_idx;

                if (self.bitmap[word_idx] & mask) != 0 {
                    // Already processed -> Duplicate
                    self.duplicates_dropped = self.duplicates_dropped.wrapping_add(1);
                    false
                } else {
                    // Valid out-of-order packet in window
                    self.bitmap[word_idx] |= mask;
                    self.accepted_out_of_order = self.accepted_out_of_order.wrapping_add(1);
                    true
                }
            }
        }
    }

    #[inline(always)]
    fn shift_window(&mut self, shift: usize) {
        if shift >= WINDOW_SIZE {
            self.bitmap.fill(0);
            return;
        }

        let word_shift = shift / 64;
        let bit_shift = shift % 64;

        for i in (0..WORDS).rev() {
            if i >= word_shift {
                let src = i - word_shift;
                let mut val = self.bitmap[src] << bit_shift;
                if bit_shift > 0 && src > 0 {
                    val |= self.bitmap[src - 1] >> (64 - bit_shift);
                }
                self.bitmap[i] = val;
            } else {
                self.bitmap[i] = 0;
            }
        }
    }

    /// Highest sequence number observed so far
    pub fn highest_seq(&self) -> Option<u32> {
        self.highest_seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_packets() {
        let mut dedup = Deduplicator::new();
        for seq in 0..5000 {
            assert!(dedup.process_packet(seq), "Seq {} should be accepted", seq);
        }
        assert_eq!(dedup.total_received, 5000);
        assert_eq!(dedup.accepted_in_order, 5000);
        assert_eq!(dedup.duplicates_dropped, 0);
        assert_eq!(dedup.highest_seq(), Some(4999));
    }

    #[test]
    fn test_exact_duplicates() {
        let mut dedup = Deduplicator::new();
        for seq in 0..100 {
            assert!(dedup.process_packet(seq));
            // Send exact duplicate
            assert!(!dedup.process_packet(seq), "Duplicate seq {} must be dropped", seq);
            // Send another duplicate
            assert!(!dedup.process_packet(seq), "Second duplicate seq {} must be dropped", seq);
        }
        assert_eq!(dedup.total_received, 300);
        assert_eq!(dedup.accepted_in_order, 100);
        assert_eq!(dedup.duplicates_dropped, 200);
    }

    #[test]
    fn test_out_of_order_within_window() {
        let mut dedup = Deduplicator::new();

        // Send seq 100 first
        assert!(dedup.process_packet(100));

        // Now send packets 50..100 in reverse order
        for seq in (50..100).rev() {
            assert!(dedup.process_packet(seq), "Out-of-order seq {} should be accepted", seq);
            // Try sending duplicate
            assert!(!dedup.process_packet(seq), "Duplicate of out-of-order seq {} must be dropped", seq);
        }

        assert_eq!(dedup.accepted_in_order, 1);
        assert_eq!(dedup.accepted_out_of_order, 50);
        assert_eq!(dedup.duplicates_dropped, 50);
    }

    #[test]
    fn test_stale_packets_beyond_1024() {
        let mut dedup = Deduplicator::new();
        assert!(dedup.process_packet(2000));

        // Seq 2000 - 1023 = 977 is still within window
        assert!(dedup.process_packet(977), "Seq 977 (diff 1023) should be within window");
        assert!(!dedup.process_packet(977), "Duplicate 977 dropped");

        // Seq 2000 - 1024 = 976 is stale (>= 1024 lag)
        assert!(!dedup.process_packet(976), "Seq 976 should be rejected as stale");
        assert!(!dedup.process_packet(500), "Seq 500 should be rejected as stale");
        assert_eq!(dedup.stale_dropped, 2);
    }

    #[test]
    fn test_sequence_number_wrapping() {
        let mut dedup = Deduplicator::new();

        // Start near u32::MAX
        let start = u32::MAX - 20;
        for i in 0..50 {
            let seq = start.wrapping_add(i);
            assert!(dedup.process_packet(seq), "Seq {} should be accepted across wrap", seq);
            assert!(!dedup.process_packet(seq), "Duplicate across wrap must be dropped");
        }

        assert_eq!(dedup.accepted_in_order, 50);
        assert_eq!(dedup.duplicates_dropped, 50);
        assert_eq!(dedup.highest_seq(), Some(start.wrapping_add(49)));
    }

    #[test]
    fn test_large_gap_forward() {
        let mut dedup = Deduplicator::new();
        assert!(dedup.process_packet(10));
        // Jump ahead by 2000 (larger than window)
        assert!(dedup.process_packet(2010));
        // Previous packet 10 is now stale
        assert!(!dedup.process_packet(10));
        // Packets in the new window work
        assert!(dedup.process_packet(2000));
        assert!(!dedup.process_packet(2000));
    }

    #[test]
    fn test_backward_sequence_resync() {
        let mut dedup = Deduplicator::new();
        // Previous session left a high sequence
        assert!(dedup.process_packet(221_726_729));

        // Client restarts and begins from seq 1
        assert!(dedup.process_packet(1), "Seq 1 after large jump backwards must trigger resync");
        assert!(dedup.process_packet(2), "Seq 2 accepted in order");
        assert!(!dedup.process_packet(1), "Seq 1 duplicate dropped");
        assert_eq!(dedup.highest_seq(), Some(2));
    }
}
