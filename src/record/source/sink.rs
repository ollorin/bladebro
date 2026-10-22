//! Bounded, non-blocking hand-off from the source to the encoder.

use crate::record::Encoder;

/// Where decoded frames go. `false` means the newest frame was refused
/// (queue full or writer gone); the source counts it and moves on, it never
/// waits (D6-5).
pub trait FrameSink: Send + Sync + 'static {
    fn send_frame(&self, jpeg: Vec<u8>, ts_ms: u64) -> bool;
}

impl FrameSink for Encoder {
    fn send_frame(&self, jpeg: Vec<u8>, ts_ms: u64) -> bool {
        Encoder::send_frame(self, jpeg, ts_ms)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn encoder_is_send_and_sync() {
        fn assert_sync<T: Send + Sync>() {}
        assert_sync::<crate::record::Encoder>();
    }
}
