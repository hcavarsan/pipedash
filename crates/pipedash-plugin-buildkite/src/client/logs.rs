use pipedash_plugin_api::http::parse_http_url;
use pipedash_plugin_api::logs::fetch::send_log;
use pipedash_plugin_api::{
    check_response,
    ContentRange,
    LogReadMode,
    LogSink,
    PluginResult,
};
use reqwest::header::{
    ACCEPT,
    ACCEPT_ENCODING,
    AUTHORIZATION,
    CONTENT_LENGTH,
    RANGE,
};
use reqwest::{
    Method,
    Url,
};

use super::BuildkiteClient;

const MAX_RESUME_ATTEMPTS: usize = 3;

impl BuildkiteClient {
    pub async fn stream_job_log(
        &self, org: &str, slug: &str, number: i64, job_id: &str, offset: Option<u64>,
        sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let url = parse_http_url(
            &format!(
                "{}/organizations/{org}/pipelines/{slug}/builds/{number}/jobs/{job_id}/log",
                self.base_url
            ),
            "Buildkite",
        )?;

        if let Some(offset) = offset {
            if let Some(next) = self.append_job_log(&url, job_id, offset, sink).await? {
                return Ok((LogReadMode::Append, next));
            }
            sink.restart()?;
        }

        let response = self
            .send_log(&url, job_id, || self.log_request(Method::GET, &url))
            .await?;
        sink.pipe(check_response(response).await?).await?;
        Ok((LogReadMode::Replace, sink.bytes_written()))
    }

    async fn append_job_log(
        &self, url: &Url, job_id: &str, offset: u64, sink: &mut LogSink,
    ) -> PluginResult<Option<u64>> {
        let head = self
            .send_log(url, job_id, || {
                self.log_request(Method::HEAD, url)
                    .header(ACCEPT_ENCODING, "identity")
            })
            .await?;
        let head = check_response(head).await?;
        let Some(size) = head
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|size| *size >= offset)
        else {
            return Ok(None);
        };
        if size == offset {
            return Ok(Some(offset));
        }

        let mut unseen = size - offset;
        for _ in 0..MAX_RESUME_ATTEMPTS {
            let response = self
                .send_log(url, job_id, || {
                    self.log_request(Method::GET, url)
                        .header(ACCEPT_ENCODING, "identity")
                        .header(RANGE, format!("bytes=-{unseen}"))
                })
                .await?;
            if response.status().as_u16() == 416 {
                return Ok(None);
            }
            let response = check_response(response).await?;
            let Some(((start, _), total)) = ContentRange::parse(response.headers())
                .and_then(|range| Some((range.span?, range.total?)))
                .filter(|(_, total)| *total >= offset && response.status().as_u16() == 206)
            else {
                return Ok(None);
            };
            if start > offset {
                unseen = total - offset;
                continue;
            }
            sink.pipe_after(response, offset - start).await?;
            return Ok(Some(offset + sink.bytes_written()));
        }
        Ok(None)
    }

    fn log_request(&self, method: Method, url: &Url) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url.clone())
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(ACCEPT, "text/plain")
    }

    async fn send_log(
        &self, url: &Url, job_id: &str, request: impl Fn() -> reqwest::RequestBuilder,
    ) -> PluginResult<reqwest::Response> {
        let gone = format!("Logs for job {job_id} are no longer available");
        send_log(&self.retry_policy, url, None, &gone, request).await
    }
}
