"""Exercise compiler/minimum and artifact identity boundaries with synthetic source metadata."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "ci_inputs", Path(__file__).parents[2] / "tools/ci_inputs.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ToolchainInputs(unittest.TestCase):
    def fixture(self, root, channel="1.23.4", components='["rustfmt", "clippy"]'):
        (root / "rust-toolchain.toml").write_text(
            f'[toolchain]\nchannel = "{channel}"\ncomponents = {components}\n'
        )
        (root / "Cargo.toml").write_text('[workspace.package]\nrust-version = "1.2"\n')

    def test_compiler_and_minimum_have_separate_authorities(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            self.assertEqual(
                MODULE.read_toolchains(root),
                {"rust": "1.23.4", "components": "rustfmt,clippy", "msrv": "1.2.0"},
            )

    def test_moving_compiler_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, channel="stable")
            with self.assertRaisesRegex(ValueError, "exact stable"):
                MODULE.read_toolchains(root)

    def test_missing_native_lint_capability_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, components='["rustfmt"]')
            with self.assertRaisesRegex(ValueError, "rustfmt/clippy"):
                MODULE.read_toolchains(root)

    def test_dependency_binary_cannot_replace_the_workspace_artifact(self):
        metadata = {
            "workspace_members": ["app"],
            "target_directory": "/isolated/target",
            "packages": [
                {"id": "dependency", "targets": [{"name": "other", "kind": ["bin"]}]},
                {"id": "app", "targets": [{"name": "actual", "kind": ["bin"]}]},
            ],
        }
        self.assertEqual(
            MODULE.release_binary(metadata), Path("/isolated/target/release/actual")
        )

    def test_ambiguous_workspace_artifacts_are_refused(self):
        metadata = {
            "workspace_members": ["app"],
            "target_directory": "/isolated/target",
            "packages": [
                {
                    "id": "app",
                    "targets": [
                        {"name": "one", "kind": ["bin"]},
                        {"name": "two", "kind": ["bin"]},
                    ],
                }
            ],
        }
        with self.assertRaisesRegex(ValueError, "exactly one"):
            MODULE.release_binary(metadata)
