const ENVIRONMENT_WORDS: [&str; 14] = [
    "dev",
    "development",
    "test",
    "testing",
    "qa",
    "uat",
    "stage",
    "staging",
    "preprod",
    "prod",
    "production",
    "live",
    "canary",
    "preview",
];

pub(super) fn name_key(name: &str) -> Option<String> {
    let kept: Vec<String> = name
        .split(|c: char| matches!(c, '-' | '_' | '.' | '/') || c.is_whitespace())
        .map(str::to_lowercase)
        .filter(|token| !token.is_empty() && !ENVIRONMENT_WORDS.contains(&token.as_str()))
        .collect();
    (!kept.is_empty()).then(|| kept.join("-"))
}
