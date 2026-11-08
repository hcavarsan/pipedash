import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from audit_structure import PROVIDERS, checks_provider, classify, inspect, provider_pattern


def rules(path, text, is_test=False, check_provider=False):
    return [violation.rule for violation in inspect(path, text, is_test, check_provider)]


class FileLinesTest(unittest.TestCase):
    def test_tsx_limit_is_300_lines(self):
        self.assertEqual(rules("src/a.tsx", "x\n" * 300), [])
        self.assertEqual(rules("src/a.tsx", "x\n" * 301), ["file-lines"])

    def test_ts_limit_is_400_lines(self):
        self.assertEqual(rules("src/a.ts", "x\n" * 400), [])
        self.assertEqual(rules("src/a.ts", "x\n" * 401), ["file-lines"])

    def test_test_code_is_exempt_from_size_and_depth(self):
        text = (" " * 40 + "x\n") * 600
        self.assertEqual(rules("crates/a/src/b.rs", text, is_test=True), [])


class DepthTest(unittest.TestCase):
    def test_typescript_depth_counts_two_space_levels(self):
        self.assertEqual(rules("src/a.ts", " " * 14 + "x\n"), [])
        self.assertEqual(rules("src/a.ts", " " * 16 + "x\n"), ["indent-depth"])

    def test_rust_depth_counts_four_space_levels(self):
        self.assertEqual(rules("crates/a/src/b.rs", " " * 28 + "x\n"), [])
        self.assertEqual(rules("crates/a/src/b.rs", " " * 32 + "x\n"), ["indent-depth"])

    def test_blank_lines_are_ignored(self):
        self.assertEqual(rules("src/a.ts", " " * 40 + "\n"), [])

    def test_tab_indentation_fails(self):
        self.assertEqual(rules("src/a.ts", "\tx\n"), ["tab-indent"])


class InlineTestsTest(unittest.TestCase):
    def test_inline_test_module_fails(self):
        self.assertEqual(rules("crates/a/src/b.rs", "mod tests {\n}\n"), ["inline-tests"])

    def test_external_test_module_passes(self):
        self.assertEqual(rules("crates/a/src/b.rs", "#[cfg(test)]\nmod tests;\n"), [])


class ProviderNameTest(unittest.TestCase):
    def test_provider_identifiers_fail(self):
        for text in ("export const isFlux = 1", "GITHUB_TOKEN", 'let id = "fluxcd__1__x";'):
            with self.subTest(text=text):
                self.assertEqual(rules("src/a.ts", text, check_provider=True), ["provider-name"])

    def test_lookalike_words_and_project_url_pass(self):
        for text in ("argon2", "prowess", "https://github.com/hcavarsan/pipedash"):
            with self.subTest(text=text):
                self.assertEqual(rules("src/a.ts", text, check_provider=True), [])

    def test_scope_excludes_provider_crates_and_frontend_tests(self):
        self.assertFalse(checks_provider("crates/pipedash-plugin-github/src/a.rs", False))
        self.assertFalse(checks_provider("crates/pipedash-plugins/src/lib.rs", False))
        self.assertTrue(checks_provider("crates/pipedash-plugin-api/src/a.rs", False))
        self.assertTrue(checks_provider("crates/pipedash-core/src/a/tests.rs", True))
        self.assertFalse(checks_provider("src/a.test.ts", True))
        self.assertTrue(checks_provider("src/a.ts", False))

    def test_providers_are_discovered_from_registering_crates(self):
        self.assertIn("pipedash-plugin-github", PROVIDERS)
        self.assertIn("flux", PROVIDERS["pipedash-plugin-fluxcd"])
        self.assertNotIn("pipedash-plugin-api", PROVIDERS)
        self.assertNotIn("pipedash-plugin-actions", PROVIDERS)

    def test_pattern_covers_hyphenated_names_aliases_and_camel_case(self):
        pattern = provider_pattern(["azure-devops", "azure", "acme"])
        for text in ("azure_devops", "AZURE-DEVOPS", "isAzureDevOps", "useAcme", "acme__1__x"):
            with self.subTest(text=text):
                self.assertIsNotNone(pattern.search(text))
        for text in ("acmes", "lacme", "Azureish"):
            with self.subTest(text=text):
                self.assertIsNone(pattern.search(text))


class ClassificationTest(unittest.TestCase):
    def test_cfg_test_module_declaration_marks_target_as_test(self):
        texts = {
            "crates/a/src/run_logs.rs": "#[cfg(test)]\nmod perf;\nmod store;\n",
            "crates/a/src/run_logs/perf.rs": "",
            "crates/a/src/run_logs/store.rs": "",
        }
        tests = classify(texts)
        self.assertIn("crates/a/src/run_logs/perf.rs", tests)
        self.assertNotIn("crates/a/src/run_logs/store.rs", tests)

    def test_cfg_all_test_through_other_attributes(self):
        texts = {
            "crates/a/src/lib.rs": "#[cfg(all(test, not(target_os = \"ios\")))]\n#[allow(dead_code)]\nmod helper;\n",
            "crates/a/src/helper.rs": "",
        }
        self.assertIn("crates/a/src/helper.rs", classify(texts))


if __name__ == "__main__":
    unittest.main()
