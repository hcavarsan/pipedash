use std::collections::BTreeSet;
use std::fs;

use crate::args::Args;
use crate::repo::{
    Repo,
    PLUGINS_MANIFEST,
};
use crate::spec::Spec;

pub(crate) const MANIFEST: &str = "[package]\nname = \"pipedash-plugins\"\n\n[dependencies]\npipedash-plugin-api.workspace = true\npipedash-plugin-alpha = { version = \"1.0.0\", path = \"../pipedash-plugin-alpha\" }\npipedash-plugin-gamma = { version = \"1.0.0\", path = \"../pipedash-plugin-gamma\" }\n\n[lints]\nworkspace = true\n";

pub(crate) fn temp_repo() -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join(PLUGINS_MANIFEST);
    fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    fs::write(&manifest, MANIFEST).unwrap();
    let repo = Repo {
        root: dir.path().to_path_buf(),
        taken: BTreeSet::new(),
        version: "1.0.0".to_string(),
    };
    (dir, repo)
}

pub(crate) fn spec(id: &str) -> Spec {
    let args = Args {
        id: Some(id.to_string()),
        description: Some("x".to_string()),
        ..Args::default()
    };
    crate::spec::from_args(&args, &BTreeSet::new()).unwrap()
}

pub(crate) fn manifest(repo: &Repo) -> String {
    fs::read_to_string(repo.root.join(PLUGINS_MANIFEST)).unwrap()
}
