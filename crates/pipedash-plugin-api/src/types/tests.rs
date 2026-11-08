use super::*;

fn permission(name: &str, required: bool) -> Permission {
    Permission {
        name: name.to_string(),
        description: String::new(),
        required,
    }
}

#[test]
fn evaluate_requires_only_the_required_permissions() {
    let permissions = [permission("read", true), permission("write", false)];
    let status = PermissionStatus::evaluate(&permissions, "pat", |p| p.name == "read");
    assert!(status.all_granted);
    assert_eq!(status.metadata["token_type"], "pat");
    assert_eq!(
        status
            .permissions
            .iter()
            .map(|c| c.granted)
            .collect::<Vec<_>>(),
        [true, false]
    );

    let status = PermissionStatus::evaluate(&permissions, "pat", |p| p.name == "write");
    assert!(!status.all_granted);
}
