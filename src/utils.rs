pub mod bitstream;

pub fn compare_streams(expected: &[u8], received: &[u8]) -> Result<(), crate::Error> {
    for (i, (a, b)) in expected.iter().zip(received.iter()).enumerate() {
        if *a != *b {
            return Err(format!(
                "mismatch at byte 0x{i:x}:\n{:x?}\n{:x?}",
                &expected[i.saturating_sub(4)..i + 8],
                &received[i.saturating_sub(4)..i + 8]
            )
            .into());
        }
    }

    if expected.len() != received.len() {
        return Err("Expected and received are different lengths".into());
    }

    Ok(())
}
