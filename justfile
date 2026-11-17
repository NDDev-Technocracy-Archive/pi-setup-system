set shell := ["bash", "-euo", "pipefail", "-c"]
export RUSTUP_AUTO_INSTALL := "0"

# Lists the commands.
default:
    @just --list

# Fetches the exact locked dependencies without compiling them.
fetch:
    cargo fetch --locked

# Prepares dependency/advisory inputs; the original audit remains mandatory.
setup: fetch
    cargo deny fetch
    cargo audit

# Runs every build-free check with prepared inputs and network access disabled.
check:
    CARGO_NET_OFFLINE=true just _check

# Checks source, policy, workflow safety and retained history.
[private]
_check:
    just --fmt --check
    python3 tools/ci_inputs.py
    cargo fmt --all --check
    cargo deny --frozen --all-features check bans licenses advisories sources
    cargo audit --no-fetch
    cargo-machete
    shfmt -d -i 2 tools/check_binary_boundary.sh
    shellcheck tools/check_binary_boundary.sh
    actionlint
    zizmor --offline --no-progress .github
    gitleaks git --no-banner --redact --log-opts=--all .

# Formats authored Rust and the command file.
fix:
    cargo fmt --all
    just --fmt
    shfmt -w -i 2 tools/check_binary_boundary.sh

# Emits the source-owned compiler and supported minimum for CI.
ci-inputs:
    python3 tools/ci_inputs.py

# Runs native lint/build/package/unit checks under one live LibClang lease.
test:
    python3 -m unittest discover -s tests/ci -v
    python3 tools/with_libclang.py just _native

# Preserves the original four native operations and their order.
[private]
_native:
    cargo clippy --locked --all-targets -- -D warnings
    just _build
    python3 tools/build_crates_io.py --self-check
    cargo test --locked --all-targets

# Builds all current targets under the declared build-only SDK.
build:
    python3 tools/with_libclang.py just _build

# Builds the source-owned targets without opening a second SDK lease.
[private]
_build:
    cargo build --locked --all-targets

# Checks the exact native release artifact, retaining only dependency caches.
boundary:
    cargo clean --release --workspace --frozen
    python3 tools/with_libclang.py cargo build --locked --release
    bash tools/check_binary_boundary.sh

# Runs the existing native test surface on macOS or Windows.
test-platform:
    python3 tools/with_libclang.py cargo test --locked --all-targets

# Compiles all targets with CI's source-declared minimum compiler.
minimum:
    python3 tools/with_libclang.py cargo check --locked --all-targets

# Runs all source/native/artifact checks for the current Linux/macOS host.
verify: check test boundary
