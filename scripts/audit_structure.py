import re
import subprocess
import sys
from pathlib import Path
from typing import NamedTuple


class Violation(NamedTuple):
    path: str
    line: int
    rule: str
    actual: int
    limit: int


SUFFIXES = (".rs", ".ts", ".tsx", ".swift", ".kt")
SKIP = re.compile(
    r"(^|/)(node_modules|target|dist|gen)/"
    r"|^crates/pipedash-desktop/gen/"
    r"|^crates/tauri-plugin-[^/]+/android/build/"
    r"|^scripts/"
    r"|\.d\.ts$"
)
TEST_PATH = re.compile(
    r"(\.test\.tsx?$|\.spec\.tsx?$|(^|/)tests?/|/tests\.rs$|/test_support\.rs$|/testing\.rs$"
    r"|/fixtures/|^src/testing/|^ios/app-store-screenshots/(tests|e2e-army)/|Tests/.*\.swift$)"
)
CRATES = Path(__file__).resolve().parents[1] / "crates"
PROVIDER_PREFIX = "pipedash-plugin-"
ALIASES = re.compile(r"^\s*aliases\s*=\s*\[([^\]]*)\]", re.MULTILINE)


def provider_crates() -> dict[str, list[str]]:
    providers: dict[str, list[str]] = {}
    for crate in sorted(CRATES.glob(f"{PROVIDER_PREFIX}*")):
        lib = crate / "src" / "lib.rs"
        if not lib.is_file() or "register_plugin!" not in lib.read_text(encoding="utf-8"):
            continue
        manifest = (crate / "Cargo.toml").read_text(encoding="utf-8")
        declared = ALIASES.search(manifest)
        aliases = re.findall(r'"([^"]+)"', declared.group(1)) if declared else []
        providers[crate.name] = [crate.name.removeprefix(PROVIDER_PREFIX), *aliases]
    return providers


def provider_pattern(terms: list[str]) -> re.Pattern[str]:
    ordered = sorted({term.lower() for term in terms}, key=len, reverse=True)
    words = [re.escape(term).replace(r"\-", "[-_]?") for term in ordered]
    camel = [term[0].upper() + "(?i:" + word[len(re.escape(term[0])):] + ")" for term, word in zip(ordered, words)]
    return re.compile(
        r"(?<![A-Za-z])(?i:" + "|".join(words) + r")(?![a-z])"
        r"|(?<=[a-z])(?:" + "|".join(camel) + r")(?![a-z])"
    )


PROVIDERS = provider_crates()
ALLOWLIST = tuple(f"crates/{name}/" for name in PROVIDERS) + (
    "crates/pipedash-plugins/",
    "crates/pipedash-core/migrations/",
)
PROJECT_URL = "github.com/hcavarsan/pipedash"
PROVIDER_NAME = provider_pattern([term for terms in PROVIDERS.values() for term in terms])
INLINE_TESTS = re.compile(r"^\s*mod tests\s*\{")
CFG_TEST = re.compile(r"^\s*#\[cfg\((test\)|all\(test)")
ATTRIBUTE = re.compile(r"^\s*#\[")
MOD_DECL = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")
FILE_LIMIT = {".tsx": 300}
DEFAULT_FILE_LIMIT = 400
DEPTH_LIMIT = 7
INDENT_UNIT = {".rs": 4, ".swift": 4, ".kt": 4, ".ts": 2, ".tsx": 2}


def inspect(path: str, text: str, is_test: bool, check_provider: bool) -> list[Violation]:
    suffix = Path(path).suffix
    lines = text.splitlines()
    found: list[Violation] = []
    limit = FILE_LIMIT.get(suffix, DEFAULT_FILE_LIMIT)
    if not is_test and len(lines) > limit:
        found.append(Violation(path, limit + 1, "file-lines", len(lines), limit))
    unit = INDENT_UNIT[suffix]
    for number, line in enumerate(lines, start=1):
        if line.startswith("\t"):
            found.append(Violation(path, number, "tab-indent", 1, 0))
        if not is_test and line.strip():
            depth = (len(line) - len(line.lstrip(" "))) // unit
            if depth > DEPTH_LIMIT:
                found.append(Violation(path, number, "indent-depth", depth, DEPTH_LIMIT))
        if suffix == ".rs" and INLINE_TESTS.match(line):
            found.append(Violation(path, number, "inline-tests", 1, 0))
    if check_provider:
        for number, line in enumerate(lines, start=1):
            if PROVIDER_NAME.search(line.replace(PROJECT_URL, "")):
                found.append(Violation(path, number, "provider-name", 1, 0))
    return found


def child_modules(path: str, text: str) -> tuple[list[str], list[str]]:
    file = Path(path)
    base = file.parent if file.stem in ("mod", "lib", "main") else file.parent / file.stem
    children: list[str] = []
    test_children: list[str] = []
    pending_test = False
    for line in text.splitlines():
        if CFG_TEST.match(line):
            pending_test = True
            continue
        declared = MOD_DECL.match(line)
        if declared:
            name = declared.group(1)
            candidates = [str(base / f"{name}.rs"), str(base / name / "mod.rs")]
            if base != file.parent:
                candidates.append(str(file.parent / f"{name}.rs"))
            children.extend(candidates)
            if pending_test:
                test_children.extend(candidates)
            pending_test = False
            continue
        if line.strip() and not ATTRIBUTE.match(line):
            pending_test = False
    return children, test_children


def is_test_path(path: str) -> bool:
    return bool(TEST_PATH.search(path))


def checks_provider(path: str, is_test: bool) -> bool:
    suffix = Path(path).suffix
    if suffix == ".rs":
        return not path.startswith(ALLOWLIST)
    if suffix in (".swift", ".kt"):
        return True
    return not is_test and (path.startswith("src/") or path.startswith("ios/app-store-screenshots/src/"))


def tracked_files() -> list[str]:
    output = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return sorted(
        path for path in set(output.splitlines())
        if path.endswith(SUFFIXES) and not SKIP.search(path) and Path(path).is_file()
    )


def classify(texts: dict[str, str]) -> set[str]:
    tests = {path for path in texts if is_test_path(path)}
    children: dict[str, list[str]] = {}
    for path, text in texts.items():
        if path.endswith(".rs"):
            all_children, test_children = child_modules(path, text)
            children[path] = all_children
            tests.update(child for child in test_children if child in texts)
    frontier = [path for path in tests if path.endswith(".rs")]
    while frontier:
        for child in children.get(frontier.pop(), []):
            if child in texts and child not in tests:
                tests.add(child)
                frontier.append(child)
    return tests


def main() -> int:
    texts = {path: Path(path).read_text(encoding="utf-8", errors="replace") for path in tracked_files()}
    tests = classify(texts)
    violations: list[Violation] = []
    for path, text in texts.items():
        is_test = path in tests
        violations.extend(inspect(path, text, is_test, checks_provider(path, is_test)))
    for violation in violations:
        print(f"{violation.path}:{violation.line}: {violation.rule} {violation.actual}>{violation.limit}")
    print(f"total: {len(violations)}")
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
