use super::*;

#[test]
fn repository_name_ignores_hosts_and_ports() {
    for url in [
        "https://github.com/acme/web.git",
        "http://gitea:3000/acme/web.git",
        "ssh://git@git.example.com:2222/acme/web",
        "git@github.com:acme/web.git",
    ] {
        assert_eq!(repository_name(url), "acme/web", "{url}");
        assert_eq!(extract_git_org(url), "acme", "{url}");
    }
}
