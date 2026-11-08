use pipedash_plugin_api::logs::tagged::{
    TaggedLogDialect,
    TAGGED_MARKERS,
};
use pipedash_plugin_api::LogDialect;

use super::*;

const STREAM: &str = concat!(
    r#"{"result":{"content":"listening on :8080","timeStampStr":"2026-10-27T10:05:01.123456789Z","podName":"web-1","last":false}}"#,
    "\n",
    r#"{"result":{"content":"container \"app\" in pod \"web-1\" is waiting to start: ContainerCreating","timeStampStr":"0001-01-01T00:00:00Z","podName":"","last":false}}"#,
    "\n",
    r#"{"result":{"content":"","timeStampStr":"2026-10-27T10:05:02Z","podName":"","last":true}}"#,
    "\n",
    r#"{"result":{"content":"after the end","timeStampStr":"2026-10-27T10:05:03Z","podName":"web-1","last":false}}"#,
    "\n",
);

fn decode(chunks: &[&[u8]]) -> PluginResult<(Vec<u8>, PodLogDecoder)> {
    let mut decoder = PodLogDecoder::default();
    let mut out = Vec::new();
    for chunk in chunks {
        decoder.feed(chunk, &mut out)?;
    }
    decoder.finish(&mut out)?;
    Ok((out, decoder))
}

#[test]
fn stream_entries_become_timestamped_lines_until_the_last_entry() {
    let (out, decoder) = decode(&[STREAM.as_bytes()]).unwrap();

    let dialect = TaggedLogDialect {
        tags: TAGGED_MARKERS,
        infer_errors: false,
    };
    let lines: Vec<_> = out
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| dialect.parse(line))
        .collect();
    assert_eq!(decoder.lines, 2);
    assert!(decoder.done);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].marker, LineMarker::Plain);
    assert_eq!(lines[0].body, b"listening on :8080");
    assert_eq!(
        lines[0].timestamp,
        Some(
            chrono::DateTime::parse_from_rfc3339("2026-10-27T10:05:01.123456789Z")
                .unwrap()
                .to_utc()
        )
    );
    assert_eq!(lines[1].marker, LineMarker::Warning);
    assert_eq!(lines[1].timestamp, None);
}

#[test]
fn entries_split_across_chunks_decode_the_same() {
    let whole = decode(&[STREAM.as_bytes()]).unwrap().0;
    for cut in [1, 17, 130, STREAM.len() - 1] {
        let (head, tail) = STREAM.as_bytes().split_at(cut);
        assert_eq!(decode(&[head, tail]).unwrap().0, whole, "cut at {cut}");
    }
}

#[test]
fn stream_errors_keep_their_http_status() {
    let error = decode(&[
        br#"{"error":{"grpc_code":7,"http_code":403,"message":"permission denied: logs, get, default/web","http_status":"Forbidden"}}"#,
    ])
    .unwrap_err();
    assert!(
        matches!(&error, PluginError::PermissionDenied(message) if message.contains("logs, get")),
        "{error:?}"
    );

    let error = decode(&[b"<html>proxy error</html>\n"]).unwrap_err();
    assert!(matches!(error, PluginError::ApiError(_)), "{error:?}");
}

#[test]
fn a_message_without_a_line_break_stops_at_the_buffer_limit() {
    let mut decoder = PodLogDecoder::default();
    let mut out = Vec::new();
    let chunk = vec![b'x'; 1024 * 1024];
    let mut fed = 0;
    let error = loop {
        match decoder.feed(&chunk, &mut out) {
            Ok(()) => fed += chunk.len(),
            Err(error) => break error,
        }
    };

    assert_eq!(fed, MAX_STREAM_MESSAGE_BYTES);
    assert!(matches!(error, PluginError::ApiError(_)), "{error:?}");
    assert!(out.is_empty());
}
