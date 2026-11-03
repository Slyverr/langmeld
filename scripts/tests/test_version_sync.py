import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from check_version_sync import check


class VersionSyncTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.write(
            "Cargo.toml",
            '[workspace]\nmembers = ["crates/langmeld-core"]\n\n'
            '[workspace.package]\nversion = "0.1.0"\n',
        )
        self.write(
            "crates/langmeld-core/Cargo.toml",
            '[package]\nname = "langmeld-core"\nversion.workspace = true\n',
        )
        self.write(
            "Cargo.lock",
            'version = 4\n\n[[package]]\nname = "langmeld-core"\nversion = "0.1.0"\n',
        )
        self.write(
            "examples/demo/Cargo.toml",
            '[package]\nname = "demo"\nversion = "0.1.0"\n\n'
            '[dependencies]\nlangmeld_core = { package = "langmeld-core", '
            'version = "0.1.0", path = "../../crates/langmeld-core" }\n\n[workspace]\n',
        )
        self.write(
            "examples/demo/Cargo.lock",
            'version = 4\n\n[[package]]\nname = "langmeld-core"\nversion = "0.1.0"\n',
        )

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, relative_path, content):
        path = self.root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def test_accepts_aligned_workspace_consumer_and_lockfiles(self):
        self.assertEqual(check(self.root), [])

    def test_accepts_a_fully_synchronized_version_bump(self):
        for relative_path in (
            "Cargo.toml",
            "Cargo.lock",
            "examples/demo/Cargo.toml",
            "examples/demo/Cargo.lock",
        ):
            path = self.root / relative_path
            path.write_text(
                path.read_text(encoding="utf-8").replace(
                    "0.1.0", "0.2.0"
                ),
                encoding="utf-8",
            )

        self.assertEqual(check(self.root), [])

    def test_rejects_stale_consumer_dependency_requirement(self):
        manifest = self.root / "examples/demo/Cargo.toml"
        manifest.write_text(
            manifest.read_text(encoding="utf-8").replace(
                'version = "0.1.0"', 'version = "0.2.0"'
            ),
            encoding="utf-8",
        )

        errors = check(self.root)

        self.assertTrue(
            any("requires '0.2.0'; expected '0.1.0'" in error for error in errors)
        )

    def test_rejects_stale_consumer_lockfile_version(self):
        lockfile = self.root / "examples/demo/Cargo.lock"
        lockfile.write_text(
            lockfile.read_text(encoding="utf-8").replace(
                'version = "0.1.0"', 'version = "0.2.0"'
            ),
            encoding="utf-8",
        )

        errors = check(self.root)

        self.assertTrue(
            any(
                "locked langmeld-core 0.2.0; expected 0.1.0" in error
                for error in errors
            )
        )

    def test_rejects_workspace_crate_that_does_not_inherit_version(self):
        manifest = self.root / "crates/langmeld-core/Cargo.toml"
        manifest.write_text(
            '[package]\nname = "langmeld-core"\nversion = "0.1.0"\n',
            encoding="utf-8",
        )

        errors = check(self.root)

        self.assertTrue(
            any("must inherit workspace.package.version" in error for error in errors)
        )


if __name__ == "__main__":
    unittest.main()
