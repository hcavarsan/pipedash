use super::ids::Digest;
use super::image::{
    ArtifactRepository,
    ImageRef,
};

const DIGEST_PREFIX: &str = "sha256:";
const DIGEST_LEN: usize = DIGEST_PREFIX.len() + 64;
const IGNORED: [&str; 6] = [
    "Pulling from",
    "Pull complete",
    "Status: Downloaded",
    "Status: Image is up to date",
    "FROM ",
    "resolve ",
];
const REF_MARKERS: [&str; 3] = ["pushing manifest for ", "Pushed ", "Published "];
const BARE_MARKERS: [&str; 2] = ["Pushed image to ", "digest: sha256:"];

pub fn pushed_digests(line: &str) -> Option<(Option<ArtifactRepository>, Digest)> {
    if IGNORED.iter().any(|ignored| line.contains(ignored)) {
        return None;
    }
    let start = line.find(DIGEST_PREFIX)?;
    let digest = Digest::parse(line.get(start..start + DIGEST_LEN)?)?;
    if line[start + DIGEST_LEN..].starts_with(|c: char| c.is_ascii_hexdigit()) {
        return None;
    }
    if let Some(repository) = pushed_reference(line) {
        return Some((Some(repository), digest));
    }
    BARE_MARKERS
        .iter()
        .any(|marker| line.contains(marker))
        .then_some((None, digest))
}

fn pushed_reference(line: &str) -> Option<ArtifactRepository> {
    let rest = REF_MARKERS
        .iter()
        .find_map(|marker| line.split_once(marker).map(|(_, rest)| rest))?;
    let token = rest.split_whitespace().next()?;
    let image = ImageRef::parse(token.trim_end_matches([',', '.', '"', '\'']))?;
    image.digest.is_some().then_some(image.repository)
}

#[cfg(test)]
mod tests;
