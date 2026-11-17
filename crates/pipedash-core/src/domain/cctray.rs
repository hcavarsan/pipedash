use std::collections::HashMap;
use std::fmt::Write;

use url::Url;

use super::pipeline::{
    Pipeline,
    PipelineStatus,
};

pub fn cctray_feed(pipelines: &[Pipeline], origin: &str) -> String {
    let base = Url::parse(origin).ok();
    let labels: Vec<String> = pipelines.iter().map(label).collect();
    let mut uses: HashMap<&str, usize> = HashMap::new();
    for label in &labels {
        *uses.entry(label).or_default() += 1;
    }
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Projects>\n");
    for (pipeline, label) in pipelines.iter().zip(&labels) {
        let name = if uses[label.as_str()] > 1 {
            let suffix = pipeline.workflow_file.as_deref().unwrap_or(&pipeline.id);
            format!("{label} · {suffix}")
        } else {
            label.clone()
        };
        project(&mut xml, pipeline, &name, base.as_ref());
    }
    xml.push_str("</Projects>\n");
    xml
}

fn label(pipeline: &Pipeline) -> String {
    if pipeline.repository.is_empty() {
        pipeline.name.clone()
    } else {
        format!("{} · {}", pipeline.repository, pipeline.name)
    }
}

fn project(xml: &mut String, pipeline: &Pipeline, name: &str, base: Option<&Url>) {
    let building = match pipeline.status {
        PipelineStatus::Running => true,
        PipelineStatus::Pending => pipeline.last_run.is_some(),
        _ => false,
    };
    let activity = if building { "Building" } else { "Sleeping" };
    let time = pipeline.last_run.unwrap_or(pipeline.last_updated);
    let _ = write!(
        xml,
        "  <Project name=\"{}\" activity=\"{activity}\" lastBuildStatus=\"{}\" lastBuildTime=\"{}\"",
        escape(name),
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
        .pop_if_empty()
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
