use chrono::TimeZone;

use super::*;

#[test]
fn rerun_object_copies_spec_and_drops_build_id() {
    let original = json!({
        "apiVersion": "prow.k8s.io/v1",
        "kind": "ProwJob",
        "metadata": {
            "name": "abc",
            "namespace": "default",
            "resourceVersion": "42",
            "labels": { "prow.k8s.io/build-id": "123", "prow.k8s.io/job": "ci-x" },
            "annotations": { "prow.k8s.io/job": "ci-x" }
        },
        "spec": { "job": "ci-x", "type": "periodic" },
        "status": { "state": "success", "build_id": "123" }
    });
    let now = Utc.with_ymd_and_hms(2026, 10, 30, 12, 0, 0).unwrap();

    let object = rerun_object(&original, now).unwrap();

    assert_eq!(object["metadata"]["generateName"], "pipedash-");
    assert!(object["metadata"].get("name").is_none());
    assert!(object["metadata"].get("resourceVersion").is_none());
    assert_eq!(
        object["metadata"]["labels"],
        json!({ "prow.k8s.io/job": "ci-x" })
    );
    assert_eq!(object["metadata"]["annotations"]["prow.k8s.io/job"], "ci-x");
    assert_eq!(object["spec"], original["spec"]);
    assert_eq!(object["status"]["state"], "triggered");
    assert_eq!(object["status"]["startTime"], "2026-10-30T12:00:00Z");
    assert!(object["status"].get("build_id").is_none());

    assert!(rerun_object(&json!({ "metadata": {} }), now).is_err());
}
