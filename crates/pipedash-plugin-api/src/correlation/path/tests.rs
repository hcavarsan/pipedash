use super::*;

#[test]
fn new_strips_leading_and_trailing_separators() {
    let cases = [
        ("./apps/api/", "apps/api"),
        ("/apps/api", "apps/api"),
        (".//apps", "apps"),
        ("apps/api", "apps/api"),
        (".", ""),
        ("./", ""),
        ("/", ""),
        ("", ""),
    ];
    for (input, expected) in cases {
        assert_eq!(PathScope::new(input).as_str(), expected, "{input}");
    }
}

#[test]
fn from_glob_keeps_the_literal_directory() {
    let cases = [
        ("services/api/**", "services/api"),
        ("services/api/*.rs", "services/api"),
        ("services/ap*/src", "services"),
        ("**/*.md", ""),
        ("*.md", ""),
        ("services/{api,web}/**", "services"),
        ("docs/readme.md", "docs/readme.md"),
    ];
    for (input, expected) in cases {
        assert_eq!(PathScope::from_glob(input).as_str(), expected, "{input}");
    }
}

#[test]
fn overlaps_is_a_segment_wise_prefix() {
    let cases = [
        ("apps/api", "apps/api", true),
        ("apps", "apps/api", true),
        ("apps/api", "apps", true),
        ("", "apps/api", true),
        ("apps/api", "apps/api-gateway", false),
        ("apps/api", "apps/web", false),
        ("app", "apps", false),
    ];
    for (left, right, expected) in cases {
        let overlaps = PathScope::new(left).overlaps(&PathScope::new(right));
        assert_eq!(overlaps, expected, "{left} vs {right}");
    }
}
