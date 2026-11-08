use std::io::Write;

use flate2::write::GzEncoder;
use flate2::Compression;

use super::*;

#[test]
fn job_list_that_unpacks_past_the_limit_is_refused() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&[b'x'; 2 << 20]).unwrap();
    let body = encoder.finish().unwrap();

    match decode_jobs(true, &body, "deck.example", 1 << 20) {
        Err(PluginError::ApiError(message)) => {
            assert!(message.contains("1 MiB unpacked limit"), "{message}");
            assert!(message.contains("deck.example"), "{message}");
        }
        other => panic!("expected the size limit error, got {other:?}"),
    }
}
