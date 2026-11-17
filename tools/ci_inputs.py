"""Read declared toolchains and native Cargo artifacts for CI without another version copy."""

from __future__ import annotations

import ast
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parent.parent
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*))?")


def read_toolchains(root: Path) -> dict[str, str]:
    toolchain = tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"]
    minimum = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"][
        "rust-version"
    ]
    channel = toolchain["channel"]
    if (
        not isinstance(channel, str)
        or VERSION.fullmatch(channel) is None
        or channel.count(".") != 2
    ):
        raise ValueError("the compiler must be an exact stable Rust version")
    if not isinstance(minimum, str) or VERSION.fullmatch(minimum) is None:
        raise ValueError("the workspace must declare its supported Rust minimum")
    components = toolchain.get("components")
    if (
        not isinstance(components, list)
        or not all(
            isinstance(value, str) and re.fullmatch(r"[a-z0-9][a-z0-9-]*", value)
            for value in components
        )
        or len(set(components)) != len(components)
        or not {"rustfmt", "clippy"}.issubset(components)
    ):
        raise ValueError(
            "the compiler input must retain unique rustfmt/clippy components"
        )
    return {
        "rust": channel,
        "components": ",".join(components),
        "msrv": minimum + ".0" if minimum.count(".") == 1 else minimum,
    }


def cargo_metadata() -> dict:
    return json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--locked",
                "--offline",
                "--no-deps",
                "--format-version",
                "1",
            ],
            cwd=ROOT,
            text=True,
        )
    )


def release_binary(metadata: dict, build_target: str | None = None) -> Path:
    members = set(metadata["workspace_members"])
    binaries = [
        target["name"]
        for package in metadata["packages"]
        if package["id"] in members
        for target in package["targets"]
        if "bin" in target["kind"]
    ]
    if len(binaries) != 1:
        raise ValueError(
            "the native boundary requires exactly one declared workspace binary"
        )
    directory = Path(metadata["target_directory"])
    if build_target:
        directory /= build_target
    return directory / "release" / binaries[0]


def main() -> None:
    if sys.argv[1:] == ["--binary"]:
        print(release_binary(cargo_metadata(), os.environ.get("CARGO_BUILD_TARGET")))
    elif not sys.argv[1:]:
        for pattern in ("tools/*.py", "tests/ci/*.py"):
            for path in sorted(ROOT.glob(pattern)):
                ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for key, value in read_toolchains(ROOT).items():
            print(f"{key}={value}")
    else:
        raise SystemExit("usage: ci_inputs.py [--binary]")


if __name__ == "__main__":
    main()
