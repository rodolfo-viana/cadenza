"""Regression tests for source discovery in the translation extractor."""
import importlib.util
import pathlib
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("cadenza_i18n", pathlib.Path(__file__).with_name("i18n.py"))
I18N = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(I18N)


class ExtractionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="cadenza-i18n-test-")
        self.original_root = I18N.ROOT
        I18N.ROOT = pathlib.Path(self.directory.name)
        (I18N.ROOT / "ui/js").mkdir(parents=True)
        (I18N.ROOT / "src-tauri/src/pptx_import").mkdir(parents=True)

    def tearDown(self):
        I18N.ROOT = self.original_root
        self.directory.cleanup()

    def write(self, name, text):
        (I18N.ROOT / name).write_text(text, encoding="utf-8")

    def test_nested_rust_messages_and_errors_are_collected(self):
        self.write("src-tauri/src/pptx_import/mod.rs", 'const MESSAGE: &str = n_("Imported image");\nanyhow!("Invalid image");')
        keys = {entry["key"]: entry for entry in I18N.collect()}
        self.assertEqual(set(keys), {"Imported image", "Invalid image"})
        self.assertEqual(keys["Imported image"]["files"], ["pptx_import/mod.rs"])

    def test_same_named_modules_keep_distinct_source_locations(self):
        self.write("src-tauri/src/mod.rs", 'n_("Shared message");')
        self.write("src-tauri/src/pptx_import/mod.rs", 'n_("Shared message");')
        self.assertEqual(I18N.collect(), [{"key": "Shared message", "files": ["mod.rs", "pptx_import/mod.rs"]}])

    def test_explicit_lowercase_labels_are_not_mistaken_for_sentinel_errors(self):
        self.write("src-tauri/src/pptx_import/mod.rs", 'n_("fonts"); n_("image"); Err("NEEDS_KEY"); Err("missing_key");')
        self.assertEqual([entry["key"] for entry in I18N.collect()], ["fonts", "image"])

    def test_test_only_strings_are_excluded_without_losing_ui_placeholders(self):
        self.write("src-tauri/src/pptx_import/mod.rs", 'n_("Production message");\n#[cfg(test)]\nmod tests { n_("Test fixture"); }')
        self.write("ui/js/example.js", "t('Slide {n}', {n: 2}); N_('Equation');")
        self.assertEqual([entry["key"] for entry in I18N.collect()], ["Equation", "Production message", "Slide {n}"])


if __name__ == "__main__":
    unittest.main()
