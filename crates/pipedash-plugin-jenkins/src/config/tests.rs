use super::*;

#[test]
fn test_encode_job_name() {
    assert_eq!(encode_job_name("folder/job"), "folder/job/job");
    assert_eq!(encode_job_name("simple"), "simple");
}

#[test]
fn test_split_job_path() {
    assert_eq!(
        split_job_path("org/repo"),
        ("org".to_string(), "repo".to_string())
    );
    assert_eq!(
        split_job_path("simple"),
        ("(root)".to_string(), "simple".to_string())
    );
}
