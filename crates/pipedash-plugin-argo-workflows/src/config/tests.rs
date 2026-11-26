use super::*;

#[test]
fn pipeline_ids_carry_namespace_kind_and_name() {
    let cron = PipelineRef::new("ci", Kind::Cron, "nightly");
    assert_eq!(cron.id(), "ci__cron__nightly");
    assert_eq!(parse_pipeline_id(&cron.id()).unwrap(), cron);
    assert_eq!(
        parse_pipeline_id("ci__template__build").unwrap(),
        PipelineRef::new("ci", Kind::Template, "build")
    );
    assert!(parse_pipeline_id("ci__build").is_err());
    assert!(parse_pipeline_id("ci__job__build").is_err());
    assert!(parse_pipeline_id("__cron__nightly").is_err());
    assert!(parse_pipeline_id("ci__cron__").is_err());
}
