use pipedash_plugin_api::content_hash;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RenderedCursor {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumes_only_when_the_old_output_is_an_unchanged_prefix() {
        let cursor = RenderedCursor::of(b". a\n. b\n");

        assert_eq!(cursor.resume(b". a\n. b\n! c\n"), Some(8));
        assert_eq!(cursor.resume(b". a\n. b\n"), Some(8));
        assert_eq!(cursor.resume(b". a\n. x\n! c\n"), None);
        assert_eq!(cursor.resume(b". a\n"), None);
    }
}
