use fluxy_proto::decode_frame;

/// Deterministic malformed-input smoke test; cargo-fuzz can reuse decode_frame.
#[test]
fn malformed_frames_never_panic() {
    for len in 0..512usize {
        for fill in [0u8, 1, 0x7f, 0x80, 0xff, 0x41] {
            let bytes = vec![fill; len];
            let _ = decode_frame(&bytes);
        }
    }

    for declared in [0u32, 1, 4, 8, u32::MAX] {
        let mut frame = declared.to_be_bytes().to_vec();
        frame.extend(std::iter::repeat_n(0x41, 32));
        let _ = decode_frame(&frame);
    }
}
