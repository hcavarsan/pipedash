use crate::application::test_support::Harness;
use crate::domain::DomainError;

const MESSAGE: &str = "Enter a whole number from 5 to 300 seconds.";

#[tokio::test]
async fn refresh_intervals_outside_five_to_three_hundred_are_rejected() {
    let h = Harness::new().await;
    let id = h.add_provider("one").await;

    for value in [i64::MIN, -1, 0, 4, 301, i64::MAX] {
        let error = h
            .provider_service
            .update_provider_refresh_interval(id, value)
            .await
            .unwrap_err();

        assert!(
            matches!(&error, DomainError::InvalidConfig(text) if text == MESSAGE),
            "{value}"
        );
    }
    assert_eq!(
        h.repository
            .get_provider(id)
            .await
            .unwrap()
            .refresh_interval,
        30
    );
}

#[tokio::test]
async fn refresh_interval_bounds_are_accepted() {
    let h = Harness::new().await;
    let id = h.add_provider("one").await;

    for value in [5, 300] {
        h.provider_service
            .update_provider_refresh_interval(id, value)
            .await
            .unwrap();
        assert_eq!(
            h.repository
                .get_provider(id)
                .await
                .unwrap()
                .refresh_interval,
            value
        );
    }
}
