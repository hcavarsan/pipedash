use super::*;

#[tokio::test]
async fn a_code_works_once_in_any_spelling() {
    let codes = PairingCodes::default();
    let (code, _) = codes.issue().unwrap();

    assert!(!codes.take("ZZZZ-ZZZZ"));
    assert!(codes.take(&format!("{}-{}", &code[..4], code[4..].to_lowercase())));
    assert!(!codes.take(&code));
}

#[tokio::test]
async fn a_code_expires_after_five_minutes() {
    let codes = PairingCodes::default();
    let (code, _) = codes.issue().unwrap();

    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(301)).await;

    assert!(!codes.take(&code));
}

#[tokio::test]
async fn the_ninth_open_code_is_refused_until_one_expires() {
    let codes = PairingCodes::default();
    for _ in 0..MAX_CODES {
        codes.issue().unwrap();
    }

    let refused = codes.issue().err().unwrap();
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(refused.error.code, "TOO_MANY_PAIRING_CODES");

    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(301)).await;

    assert!(codes.issue().is_ok());
}
