use std::borrow::Cow;
use std::io;
use std::ops::Range;
use std::thread;

use regex::{
    Regex,
    RegexBuilder,
};
use tokio_util::sync::CancellationToken;

use super::{
    DocError,
    Document,
};
use crate::domain::{
    DomainError,
    DomainResult,
    LogSearchQuery,
    LogSearchResult,
};
use crate::infrastructure::run_logs::{
    SEARCH_MATCH_CAP,
    SEARCH_MAX_QUERY_CHARS,
    SEARCH_REGEX_SIZE_LIMIT,
};

const MAX_SEARCH_WORKERS: usize = 4;

pub fn search_regex(query: &LogSearchQuery) -> DomainResult<Option<Regex>> {
    if query.text.is_empty() {
        return Ok(None);
    }
    if query.text.chars().nth(SEARCH_MAX_QUERY_CHARS).is_some() {
        return Err(DomainError::InvalidConfig("Search text is too long".into()));
    }
    let pattern = if query.regex {
        Cow::Borrowed(query.text.as_str())
    } else {
        Cow::Owned(regex::escape(&query.text))
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!query.case_sensitive)
        .size_limit(SEARCH_REGEX_SIZE_LIMIT)
        .build()
        .map(Some)
        .map_err(|e| DomainError::InvalidConfig(format!("Invalid search pattern: {e}")))
}

pub(super) struct Partial {
    matches: Vec<(u32, u32, u32)>,
    total: u64,
}

impl Document {
    pub fn search(
        &self, regex: &Regex, cancel: &CancellationToken,
    ) -> Result<LogSearchResult, DocError> {
        let total_blocks = self.blocks.len();
        if total_blocks == 0 {
            return Ok(LogSearchResult::default());
        }
        let workers = thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .min(MAX_SEARCH_WORKERS)
            .min(total_blocks);
        let per_worker = total_blocks.div_ceil(workers);
        let partials: Vec<Result<Partial, DocError>> = thread::scope(|scope| {
            let handles: Vec<_> = (0..workers)
                .map(|worker| {
                    let range = (worker * per_worker).min(total_blocks)
                        ..((worker + 1) * per_worker).min(total_blocks);
                    let regex = regex.clone();
                    scope.spawn(move || self.search_range(range, &regex, cancel))
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err(io::Error::other("Search worker panicked").into()))
                })
                .collect()
        });

        let mut matches = Vec::new();
        let mut total = 0u64;
        for partial in partials {
            let partial = partial?;
            total += partial.total;
            let room = SEARCH_MATCH_CAP.saturating_sub(matches.len());
            matches.extend(partial.matches.into_iter().take(room));
        }
        Ok(LogSearchResult {
            matches,
            total,
            capped: total > SEARCH_MATCH_CAP as u64,
        })
    }

    fn search_range(
        &self, range: Range<usize>, regex: &Regex, cancel: &CancellationToken,
    ) -> Result<Partial, DocError> {
        let mut partial = Partial {
            matches: Vec::new(),
            total: 0,
        };
        let mut file = None;
        let mut scratch = Vec::new();
        for index in range {
            if cancel.is_cancelled() {
                return Err(DocError::Cancelled);
            }
            let block = self.load_block(&mut file, &mut scratch, index)?;
            let first_line = self.blocks[index].first_line;
            for (offset, meta) in block.lines.iter().enumerate() {
                search_line(
                    regex,
                    block.line_text(meta),
                    first_line + offset as u32,
                    &mut partial,
                );
            }
        }
        Ok(partial)
    }
}

pub(super) fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

fn search_line(regex: &Regex, text: &str, line: u32, partial: &mut Partial) {
    let mut ascii: Option<bool> = None;
    let mut last_byte = 0usize;
    let mut last_units = 0u32;
    for found in regex.find_iter(text) {
        if found.is_empty() {
            continue;
        }
        partial.total += 1;
        if partial.matches.len() >= SEARCH_MATCH_CAP {
            continue;
        }
        let ascii = *ascii.get_or_insert_with(|| text.is_ascii());
        let (start, end) = if ascii {
            (found.start() as u32, found.end() as u32)
        } else {
            let start = last_units + utf16_len(&text[last_byte..found.start()]);
            let end = start + utf16_len(found.as_str());
            last_byte = found.end();
            last_units = end;
            (start, end)
        };
        partial.matches.push((line, start, end));
    }
}
