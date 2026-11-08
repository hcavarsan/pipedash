use std::collections::BTreeSet;
use std::future::Future;

use futures::stream::{
    self,
    StreamExt,
    TryStreamExt,
};
use serde::de::DeserializeOwned;

use crate::error::{
    PluginError,
    PluginResult,
};
use crate::health::ClientOptions;
use crate::logs::{
    LogCursor,
    LogSink,
    RunRef,
};
use crate::types::{
    AvailablePipeline,
    Organization,
    TriggerParams,
    WorkflowParameter,
    WorkflowParameterType,
};

pub const CONCURRENCY: usize = 8;
pub const LOG_CHUNK_BYTES: usize = 64 * 1024;

pub fn run_metadata_str<'a>(run: &RunRef<'a>, key: &str) -> Option<&'a str> {
    run.metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
}

pub fn client_options(origin: Option<&str>, insecure: bool) -> ClientOptions {
    ClientOptions {
        origin: origin.and_then(|url| reqwest::Url::parse(url).ok()),
        accept_invalid_certs: insecure,
        ..ClientOptions::default()
    }
}

pub fn input_str<'a>(params: &'a TriggerParams, key: &str) -> Option<&'a str> {
    params
        .inputs
        .as_ref()
        .and_then(|inputs| inputs.get(key))
        .and_then(serde_json::Value::as_str)
}

pub fn string_parameter(
    name: &str, label: &str, description: &str, default: Option<String>, required: bool,
) -> WorkflowParameter {
    WorkflowParameter {
        name: name.to_string(),
        label: Some(label.to_string()),
        description: Some(description.to_string()),
        param_type: WorkflowParameterType::String { default },
        required,
    }
}

pub async fn write_in_chunks<I>(
    sink: &mut LogSink, items: impl IntoIterator<Item = I>,
    mut render: impl FnMut(&mut Vec<u8>, I) -> PluginResult<()>,
) -> PluginResult<()> {
    let mut chunk = Vec::new();
    for item in items {
        render(&mut chunk, item)?;
        if chunk.len() >= LOG_CHUNK_BYTES {
            sink.write(std::mem::take(&mut chunk).into()).await?;
        }
    }
    sink.write(chunk.into()).await
}

pub fn organizations<'a>(
    names: impl IntoIterator<Item = &'a str>, description: Option<&str>,
) -> Vec<Organization> {
    names
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|name| Organization {
            id: name.to_string(),
            name: name.to_string(),
            description: description.map(str::to_string),
        })
        .collect()
}

pub fn require_configured<T>(items: Vec<T>, what: &str) -> PluginResult<Vec<T>> {
    if items.is_empty() {
        return Err(PluginError::InvalidConfig(format!("No {what} configured")));
    }
    Ok(items)
}

pub fn retain_matching(items: &mut Vec<AvailablePipeline>, search: &str) {
    let search = search.to_lowercase();
    items.retain(|pipeline| {
        pipeline.name.to_lowercase().contains(&search)
            || pipeline.id.to_lowercase().contains(&search)
            || pipeline
                .description
                .as_ref()
                .is_some_and(|description| description.to_lowercase().contains(&search))
    });
}

pub async fn join_buffered<F: Future>(requests: Vec<F>, limit: usize) -> Vec<F::Output> {
    stream::iter(requests).buffered(limit).collect().await
}

pub async fn try_buffered<F, T>(requests: Vec<F>, limit: usize) -> PluginResult<Vec<T>>
where
    F: Future<Output = PluginResult<T>>,
{
    stream::iter(requests).buffered(limit).try_collect().await
}

pub async fn collect_tolerant<F, T>(
    requests: Vec<F>, limit: usize, mut on_error: impl FnMut(&PluginError),
) -> PluginResult<Vec<T>>
where
    F: Future<Output = PluginResult<T>>,
{
    let results: Vec<_> = stream::iter(requests).buffered(limit).collect().await;
    let mut items = Vec::new();
    let mut first_error = None;

    for result in results {
        match result {
            Ok(item) => items.push(item),
            Err(error) if error.is_transient() => return Err(error),
            Err(error) => {
                on_error(&error);
                first_error.get_or_insert(error);
            }
        }
    }

    match first_error {
        Some(error) if items.is_empty() => Err(error),
        _ => Ok(items),
    }
}

pub fn resume_position<T: DeserializeOwned>(
    from: Option<&LogCursor>, sink: &mut LogSink, keep: impl FnOnce(&T) -> bool,
) -> PluginResult<Option<T>> {
    let position = from
        .and_then(|cursor| cursor.parse::<T>())
        .filter(|position| keep(position));
    if from.is_some() && position.is_none() {
        sink.restart()?;
    }
    Ok(position)
}

pub fn digit_id<'a>(id: &'a str, what: &str) -> PluginResult<&'a str> {
    checked_id(id, what, |byte| byte.is_ascii_digit())
}

pub fn numeric_id<T: std::str::FromStr>(id: &str, what: &str) -> PluginResult<T> {
    digit_id(id, what)?
        .parse()
        .map_err(|_| PluginError::InvalidConfig(format!("Invalid {what}: {id}")))
}

pub fn uuid_id<'a>(id: &'a str, what: &str) -> PluginResult<&'a str> {
    checked_id(id, what, |byte| byte.is_ascii_hexdigit() || byte == b'-')
}

fn checked_id<'a>(id: &'a str, what: &str, valid: fn(u8) -> bool) -> PluginResult<&'a str> {
    if id.is_empty() || !id.bytes().all(valid) {
        return Err(PluginError::InvalidConfig(format!("Invalid {what}: {id}")));
    }
    Ok(id)
}

#[cfg(test)]
mod tests;
