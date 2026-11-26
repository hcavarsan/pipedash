use super::StageRun;
use crate::application::services::correlation_service::copy;
use crate::domain::application::dto::{
    SegmentKind,
    TimeSegment,
};

fn segment(kind: SegmentKind, seconds: i64) -> TimeSegment {
    TimeSegment {
        kind,
        seconds,
        label: copy::segment_label(kind, seconds),
    }
}

pub(in crate::application::services::correlation_service) fn segments(
    build: Option<&StageRun>, deploy: Option<&StageRun>,
) -> Vec<TimeSegment> {
    let build_span = build.and_then(StageRun::timed_span);
    let deploy_span = deploy
        .filter(|run| run.attempt)
        .and_then(StageRun::timed_span);
    let deployed_at = deploy.and_then(StageRun::succeeded_at);
    let mut out = Vec::new();
    if let Some((started, ended)) = build_span {
        out.push(segment(SegmentKind::Build, (ended - started).num_seconds()));
    }
    match (build_span.map(|(_, ended)| ended), deploy_span, deployed_at) {
        (Some(built), Some((started, ended)), _) if started >= built => {
            out.push(segment(SegmentKind::Wait, (started - built).num_seconds()));
            out.push(segment(
                SegmentKind::Deploy,
                (ended - started).num_seconds(),
            ));
        }
        (Some(built), _, Some(deployed)) if deployed >= built => {
            out.push(segment(
                SegmentKind::BuildToDeployed,
                (deployed - built).num_seconds(),
            ));
        }
        (None, Some((started, ended)), _) => {
            out.push(segment(
                SegmentKind::Deploy,
                (ended - started).num_seconds(),
            ));
        }
        _ => {}
    }
    out
}
