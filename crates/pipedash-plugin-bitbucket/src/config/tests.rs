use super::*;

#[test]
fn status_source_is_the_bitbucket_statuspage_feed() {
    let source = status_source().unwrap();
    assert_eq!(source.feed_url.path(), "/api/v2/summary.json");
    assert_eq!(source.format, FeedFormat::Statuspage);
    assert!(matches!(source.components, ComponentFilter::Ids(ids) if ids.len() == 4));
}
