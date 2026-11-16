use std::fmt::Write;

use url::Url;

use super::pipeline::{
    Pipeline,
    PipelineStatus,
};

pub fn cctray_feed(pipelines: &[Pipeline], origin: &str) -> String {
    let base = Url::parse(origin).ok();
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Projects>\n");
    for pipeline in pipelines {
        project(&mut xml, pipeline, base.as_ref());
    }
    xml.push_str("</Projects>\n");
    xml
}

fn project(xml: &mut String, pipeline: &Pipeline, base: Option<&Url>) {
    let name = if pipeline.repository.is_empty() {
        pipeline.name.clone()
    } else {
        format!("{} · {}", pipeline.repository, pipeline.name)
    };
    let activity = if pipeline.status.is_terminal() {
        "Sleeping"
    } else {
        "Building"
    };
    let time = pipeline.last_run.unwrap_or(pipeline.last_updated);
    let _ = write!(
        xml,
        "  <Project name=\"{}\" activity=\"{activity}\" lastBuildStatus=\"{}\" lastBuildTime=\"{}\"",
        escape(&name),
        last_build_status(&pipeline.status),
        time.to_rfc3339(),
    );
    if let Some(url) = base.and_then(|base| pipeline_url(base, &pipeline.id)) {
        let _ = write!(xml, " webUrl=\"{}\"", escape(url.as_str()));
    }
    xml.push_str("/>\n");
}

fn last_build_status(status: &PipelineStatus) -> &'static str {
    match status {
        PipelineStatus::Success => "Success",
        PipelineStatus::Failed | PipelineStatus::Degraded => "Failure",
        PipelineStatus::Running
        | PipelineStatus::Pending
        | PipelineStatus::Cancelled
        | PipelineStatus::Skipped => "Unknown",
    }
}

fn pipeline_url(base: &Url, id: &str) -> Option<Url> {
    let mut url = base.clone();
    url.path_segments_mut()
        .ok()?
        .clear()
        .extend(["pipelines", id]);
    Some(url)
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests;
