//! Bounded, non-blocking hand-off from the source to the encoder.

use crate::record::Encoder;

/// Where decoded frames go. `false` means the newest frame was refused
/// (queue full or writer gone); the source counts it and moves on, it never
/// waits (D6-5).
pub trait FrameSink: Send + Sync + 'static {
    fn send_frame(&self, jpeg: Vec<u8>, ts_ms: u64) -> bool;

    /// The recording's end is now (stop was requested). The encoder holds the
    /// last frame until this instant rather than until `finish`, which runs
    /// only after the stop grace and would otherwise add it to the video.
    fn mark_end(&self) {}
}

impl FrameSink for Encoder {
    fn send_frame(&self, jpeg: Vec<u8>, ts_ms: u64) -> bool {
        Encoder::send_frame(self, jpeg, ts_ms)
    }

    fn mark_end(&self) {
        Encoder::mark_end(self)
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
