pub mod bitstream;

pub(crate) fn compare_streams(expected: &[u8], received: &[u8]) {
    for (i, (a, b)) in expected.iter().zip(received.iter()).enumerate() {
        assert_eq!(
            *a,
            *b,
            "mismatch at byte 0x{i:x}:\n{:x?}\n{:x?}",
            &expected[i.saturating_sub(4)..i + 8],
            &received[i.saturating_sub(4)..i + 8]
        );
    }

    assert_eq!(
        expected.len(),
        received.len(),
        "Expected and received are different lengths"
    );
}
