use pipedash_core::infrastructure::push_relay::RelayOutcome;

use crate::test_support::{
    hosted,
    stub_server,
};

#[tokio::test]
async fn a_stale_gone_answer_keeps_the_registration_that_replaced_it() {
    let host = hosted("gone-replaced").await;
    let relay = stub_server().await;
    relay.reply_with(410);
    let core = host.state.core().await.unwrap();
    let (device, _) = core.pair_device("Pixel", "ios").await.unwrap();
    core.device_pushes().set(&device.id, "sub-a").await.unwrap();
    let stale = core.device_pushes().get(&device.id).await.unwrap().unwrap();
    core.device_pushes().set(&device.id, "sub-b").await.unwrap();

    let service = core.push_service().with_fixed_relay(Some(&relay.url));
    let outcome = service.deliver_alert(&stale, "a1", None).await.unwrap();

    assert_eq!(outcome, RelayOutcome::Gone);
    assert_eq!(relay.requests().len(), 1);
    let kept = core.device_pushes().get(&device.id).await.unwrap().unwrap();
    assert_eq!(kept.relay_subscription_id, "sub-b");

    let current = kept;
    service.deliver_alert(&current, "a2", None).await.unwrap();
    assert!(core
        .device_pushes()
        .get(&device.id)
        .await
        .unwrap()
        .is_none());
}
