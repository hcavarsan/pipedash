use futures::{
    StreamExt,
    TryStreamExt,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use reqwest::Url;

use super::{
    Artifacts,
    HistoryRoot,
    StorageClient,
    StoredBuild,
    MIN_LISTING_FLOOR,
};
use crate::config::Bucket;
use crate::types::{
    run_number_of,
    snowflake_span,
    Finished,
    ObjectList,
    Started,
};

const CONCURRENCY: usize = 8;

impl StorageClient {
    pub(crate) async fn build_numbers(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, from: Option<i64>, until: Option<i64>,
    ) -> PluginResult<Vec<i64>> {
        let Some(base) = self
            .base(&bucket.provider)
            .filter(|_| bucket.provider == "gs")
        else {
            return Ok(Vec::new());
        };
        let list_url = Url::parse(&format!("{base}/storage/v1/b/{}/o", bucket.name))
            .map_err(|e| PluginError::InvalidConfig(format!("Invalid storage URL: {e}")))?;
        let prefix = format!("{}/", root.key(job));
        let lexical = from.is_some_and(|from| from >= MIN_LISTING_FLOOR);
        let offset = |bound: Option<i64>| {
            bound
                .filter(|_| lexical)
                .map(|bound| format!("{prefix}{bound}"))
        };
        let (start_offset, end_offset) = (offset(from), offset(until));
        let mut numbers: Vec<i64> = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = list_url.clone();
            {
                let mut query = url.query_pairs_mut();
                query
                    .append_pair("prefix", &prefix)
                    .append_pair("maxResults", "1000");
                match root {
                    HistoryRoot::Logs => query
                        .append_pair("delimiter", "/")
                        .append_pair("fields", "prefixes,nextPageToken"),
                    HistoryRoot::PullDirectory => {
                        query.append_pair("fields", "items(name),nextPageToken")
                    }
                };
                if let Some(start) = &start_offset {
                    query.append_pair("startOffset", start);
                }
                if let Some(end) = &end_offset {
                    query.append_pair("endOffset", end);
                }
                if let Some(token) = &page_token {
                    query.append_pair("pageToken", token);
                }
            }
            let Some(list) = self.read_json::<ObjectList>(url).await? else {
                break;
            };
            match root {
                HistoryRoot::Logs => numbers.extend(
                    list.prefixes
                        .iter()
                        .filter_map(|prefix| prefix.trim_end_matches('/').rsplit('/').next())
                        .filter_map(|name| name.parse::<i64>().ok()),
                ),
                HistoryRoot::PullDirectory => numbers.extend(
                    list.items
                        .iter()
                        .filter_map(|item| item.name.strip_suffix(".txt"))
                        .filter_map(|name| name.rsplit('/').next())
                        .filter_map(|name| name.parse::<i64>().ok()),
                ),
            }
            match list.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }

        numbers.retain(|number| {
            *number > 0
                && from.is_none_or(|from| *number >= from)
                && until.is_none_or(|until| *number < until)
        });
        numbers.sort_unstable_by(|a, b| b.cmp(a));
        numbers.dedup();
        Ok(numbers)
    }

    pub(crate) async fn find_build(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, run_number: i64,
    ) -> PluginResult<Option<StoredBuild>> {
        let span = snowflake_span(run_number);
        let bounded = span.is_some_and(|(start, _)| start >= MIN_LISTING_FLOOR);
        if bounded {
            if let Some(build_id) = self
                .listed_build(bucket, root, job, run_number, span)
                .await?
            {
                return self.build(bucket, root, job, build_id).await;
            }
        }
        if run_number_of(run_number) == run_number {
            if let Some(build) = self.build(bucket, root, job, run_number).await? {
                return Ok(Some(build));
            }
        }
        if bounded {
            return Ok(None);
        }
        match self
            .listed_build(bucket, root, job, run_number, span)
            .await?
        {
            Some(build_id) => self.build(bucket, root, job, build_id).await,
            None => Ok(None),
        }
    }

    async fn listed_build(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, run_number: i64,
        span: Option<(i64, i64)>,
    ) -> PluginResult<Option<i64>> {
        let Some((start, end)) = span else {
            return Ok(None);
        };
        Ok(self
            .build_numbers(bucket, root, job, Some(start), Some(end))
            .await?
            .into_iter()
            .find(|build_id| run_number_of(*build_id) == run_number))
    }

    pub(crate) async fn build(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, number: i64,
    ) -> PluginResult<Option<StoredBuild>> {
        let directory = match root {
            HistoryRoot::Logs => format!("{}/{number}", root.key(job)),
            HistoryRoot::PullDirectory => {
                let Some(url) = self.object_url(
                    &bucket.provider,
                    &bucket.name,
                    &format!("{}/{number}.txt", root.key(job)),
                ) else {
                    return Ok(None);
                };
                let Some(link) = self.read_object(url).await? else {
                    return Ok(None);
                };
                let link = String::from_utf8_lossy(&link);
                let Some(directory) = link
                    .trim()
                    .strip_prefix(&format!("{}://{}/", bucket.provider, bucket.name))
                else {
                    return Ok(None);
                };
                directory.trim_end_matches('/').to_string()
            }
        };
        let (Some(started_url), Some(finished_url)) = (
            self.object_url(
                &bucket.provider,
                &bucket.name,
                &format!("{directory}/started.json"),
            ),
            self.object_url(
                &bucket.provider,
                &bucket.name,
                &format!("{directory}/finished.json"),
            ),
        ) else {
            return Ok(None);
        };
        let (started, finished) = futures::try_join!(
            self.read_json::<Started>(started_url),
            self.read_json::<Finished>(finished_url)
        )?;
        let (Some(started), Some(finished)) = (started, finished) else {
            return Ok(None);
        };
        Ok(Some(StoredBuild {
            number,
            artifacts: Artifacts {
                provider: bucket.provider.clone(),
                bucket: bucket.name.clone(),
                path: directory,
            },
            started,
            finished,
        }))
    }

    pub(crate) async fn latest_build(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str,
    ) -> PluginResult<Option<StoredBuild>> {
        let Some(url) = self.object_url(
            &bucket.provider,
            &bucket.name,
            &format!("{}/latest-build.txt", root.key(job)),
        ) else {
            return Ok(None);
        };
        let Some(body) = self.read_object(url).await? else {
            return Ok(None);
        };
        match String::from_utf8_lossy(&body).trim().parse::<i64>() {
            Ok(number) if number > 0 => self.build(bucket, root, job, number).await,
            _ => Ok(None),
        }
    }

    pub(crate) async fn builds(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, numbers: Vec<i64>,
    ) -> PluginResult<Vec<StoredBuild>> {
        let builds: Vec<Option<StoredBuild>> = futures::stream::iter(numbers)
            .map(|number| self.build(bucket, root, job, number))
            .buffered(CONCURRENCY)
            .try_collect()
            .await?;
        Ok(builds.into_iter().flatten().collect())
    }
}
