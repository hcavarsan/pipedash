use pipedash_plugin_api::http::parse_http_url;
use pipedash_plugin_api::logs::fetch::send_log;
use pipedash_plugin_api::{
    check_response,
    LogCursor,
    LogRead,
    LogSink,
    PluginError,
    PluginResult,
};
use reqwest::Method;

use super::JenkinsClient;
use crate::{
    config,
    types,
};

impl JenkinsClient {
    pub async fn stream_console_log(
        &self, job_path: &str, build_number: i64, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let resume = match from {
            None => None,
            Some(cursor) => {
                let known = cursor
                    .parse::<types::ConsoleCursor>()
                    .filter(|cursor| cursor.build == build_number);
                if known.is_none() {
                    sink.restart()?;
                }
                known.map(|cursor| cursor.offset)
            }
        };
        let encoded_path = config::encode_job_name(job_path);
        let url = parse_http_url(
            &format!(
                "{}/job/{}/{}/logText/progressiveText?start={}",
                self.server_url,
                encoded_path,
                build_number,
                resume.unwrap_or(0)
            ),
            "Jenkins",
        )?;

        let gone = format!("Console log of build #{build_number} is no longer available");
        let response = send_log(&self.retry_policy, &url, None, &gone, || {
            self.authorized(Method::GET, &url)
        })
        .await?;

        let response = check_response(response).await?;
        let size = response
            .headers()
            .get("X-Text-Size")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok());
        match (resume, size) {
            (Some(_), None) => {
                return Err(PluginError::ApiError(
                    "Jenkins did not report the console text size".into(),
                ))
            }
            (Some(start), Some(size)) if size < start => sink.restart()?,
            _ => {}
        }

        sink.pipe(response).await?;

        let cursor = size
            .map(|offset| {
                LogCursor::new(&types::ConsoleCursor {
                    build: build_number,
                    offset,
                })
            })
            .transpose()?;
        Ok(match cursor {
            Some(cursor) if resume.is_some() && !sink.restarted() => LogRead::append(cursor),
            cursor => LogRead::replace(cursor),
        })
    }
}
