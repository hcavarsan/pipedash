use bytes::Bytes;
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    content_hash,
    LogCursor,
    LogRead,
    LogSink,
};
use crate::error::PluginResult;

#[derive(Debug, Serialize, Deserialize)]
pub struct RenderedCursor {
    length: usize,
    hash: u64,
}

impl RenderedCursor {
    pub fn of(rendered: &[u8]) -> Self {
        Self {
            length: rendered.len(),
            hash: content_hash(rendered),
        }
    }

    pub fn resume(&self, rendered: &[u8]) -> Option<usize> {
        let prefix = rendered.get(..self.length)?;
        (content_hash(prefix) == self.hash).then_some(self.length)
    }

    pub async fn read(
        rendered: Bytes, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let resumed = match from {
            None => None,
            Some(cursor) => {
                let resumed = cursor
                    .parse::<RenderedCursor>()
                    .and_then(|cursor| cursor.resume(&rendered));
                if resumed.is_none() {
                    sink.restart()?;
                }
                resumed
            }
        };
        let next = LogCursor::new(&RenderedCursor::of(&rendered))?;
        match resumed {
            Some(offset) => {
                sink.write_chunks(rendered.slice(offset..)).await?;
                Ok(LogRead::append(next))
            }
            None => {
                sink.write_chunks(rendered).await?;
                Ok(LogRead::replace(Some(next)))
            }
        }
    }
}

#[cfg(test)]
mod tests;
