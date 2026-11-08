use bytes::Bytes;
use pipedash_plugin_api::logs::fetch::send_log;
use pipedash_plugin_api::{
    next_log_chunk,
    LogCursor,
    LogRead,
    LogSink,
    PluginResult,
};
use reqwest::header::ACCEPT;
use reqwest::Method;

use super::{
    signed_in,
    AzureDevOpsClient,
};
use crate::log_tail::{
    BoundaryScan,
    LogPosition,
    Scan,
    Tally,
};

impl AzureDevOpsClient {
    pub async fn stream_build_log(
        &self, project: &str, build_id: i64, log_id: i64, attempt: i64, from: Option<&LogCursor>,
        sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let mut resume = match from {
            None => None,
            Some(cursor) => {
                let known = cursor
                    .parse::<LogPosition>()
                    .filter(|position| position.is_for(build_id, log_id, attempt));
                if known.is_none() {
                    sink.restart()?;
                }
                known
            }
        };
        let mut opened = None;
        if let Some(position) = resume.as_mut() {
            opened = self
                .open_after_boundary(project, build_id, log_id, position)
                .await?;
            if opened.is_none() {
                sink.restart()?;
                resume = None;
            }
        }
        let (mut response, rest) = match opened {
            Some(opened) => opened,
            None => (
                self.open_build_log(project, build_id, log_id, None).await?,
                Bytes::new(),
            ),
        };

        let mut tally = resume.as_ref().map_or_else(Tally::default, Tally::resume);
        let mut eol = resume.as_ref().is_some_and(|position| position.open);
        let mut chunk = Some(rest);
        while let Some(bytes) = chunk {
            if !bytes.is_empty() {
                if std::mem::take(&mut eol) {
                    sink.write(Bytes::from_static(b"\n")).await?;
                }
                tally.feed(&bytes);
                sink.write(bytes).await?;
            }
            chunk = next_log_chunk(&mut response).await?;
        }

        let appended = resume.is_some();
        let next = match resume {
            Some(position) if !tally.fed() => position,
            _ => tally.position(build_id, log_id, attempt),
        };
        let cursor = LogCursor::new(&next)?;
        Ok(if appended {
            LogRead::append(cursor)
        } else {
            LogRead::replace(Some(cursor))
        })
    }

    async fn open_after_boundary(
        &self, project: &str, build_id: i64, log_id: i64, position: &mut LogPosition,
    ) -> PluginResult<Option<(reqwest::Response, Bytes)>> {
        if position.lines == 0 {
            let response = self.open_build_log(project, build_id, log_id, None).await?;
            return Ok(Some((response, Bytes::new())));
        }
        if position.first_line.is_none() {
            let Some(start) = position.probe_line() else {
                return Ok(None);
            };
            let mut probe = self
                .open_build_log(project, build_id, log_id, Some(start))
                .await?;
            match first_line_hash(&mut probe).await? {
                Some((hash, _)) if position.settle(hash) => {}
                _ => return Ok(None),
            }
        }
        let Some(start) = position.boundary_line() else {
            return Ok(None);
        };
        let mut response = self
            .open_build_log(project, build_id, log_id, Some(start))
            .await?;
        Ok(match first_line_hash(&mut response).await? {
            Some((hash, rest)) if hash == position.last => Some((response, rest)),
            _ => None,
        })
    }

    async fn open_build_log(
        &self, project: &str, build_id: i64, log_id: i64, start_line: Option<u64>,
    ) -> PluginResult<reqwest::Response> {
        let path = format!("_apis/build/builds/{build_id}/logs/{log_id}");
        let query: Vec<_> = start_line
            .map(|line| ("startLine", line.to_string()))
            .into_iter()
            .collect();
        let url = self.url(Some(project), &path, &query)?;
        let gone = format!("Log {log_id} of build {build_id} is no longer available");
        let response = send_log(&self.retry_policy, &url, None, &gone, || {
            self.authorized(Method::GET, &url)
                .header(ACCEPT, "text/plain")
        })
        .await?;
        signed_in(response).await
    }
}

async fn first_line_hash(response: &mut reqwest::Response) -> PluginResult<Option<(u64, Bytes)>> {
    let mut scan = BoundaryScan::new();
    while let Some(chunk) = next_log_chunk(response).await? {
        match scan.feed(chunk) {
            Scan::More => {}
            Scan::Line(hash, rest) => return Ok(Some((hash, rest))),
            Scan::TooLong => return Ok(None),
        }
    }
    Ok(scan.finish().map(|hash| (hash, Bytes::new())))
}
