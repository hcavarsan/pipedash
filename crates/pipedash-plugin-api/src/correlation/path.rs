use serde::{
    Deserialize,
    Serialize,
};

const GLOB_CHARS: [char; 4] = ['*', '?', '[', '{'];

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PathScope(String);

impl PathScope {
    pub fn new(path: &str) -> Self {
        let mut path = path.trim();
        loop {
            let stripped = path
                .strip_prefix("./")
                .unwrap_or(path)
                .trim_start_matches('/');
            if stripped == path {
                break;
            }
            path = stripped;
        }
        let path = path.trim_end_matches('/');
        Self(if path == "." {
            String::new()
        } else {
            path.to_string()
        })
    }

    pub fn from_glob(glob: &str) -> Self {
        let Some(first) = glob.find(GLOB_CHARS) else {
            return Self::new(glob);
        };
        let literal = &glob[..first];
        Self::new(literal.rfind('/').map_or("", |slash| &literal[..slash]))
    }

    pub fn overlaps(&self, other: &PathScope) -> bool {
        let (short, long) = if self.0.len() <= other.0.len() {
            (&self.0, &other.0)
        } else {
            (&other.0, &self.0)
        };
        if short.is_empty() {
            return true;
        }
        long.strip_prefix(short.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests;
