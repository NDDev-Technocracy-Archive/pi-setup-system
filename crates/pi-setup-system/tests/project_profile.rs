//! The project profile consumed by ai-stp must describe vendor-loaded paths.

#![allow(
    clippy::expect_used,
    reason = "integration fixtures require Cargo's binary"
)]

use std::process::Command;

use serde_json::Value;

fn owns(namespaces: &[Value], path: &str) -> bool {
    namespaces.iter().filter_map(Value::as_str).any(|root| {
        path == root
            || path
                .strip_prefix(root)
                .is_some_and(|tail| tail.starts_with('/'))
    })
}

#[test]
fn public_project_profile_routes_supported_kinds_without_claiming_runtime_or_credentials() {
    let executable = std::env::var("CARGO_BIN_EXE_pi-setup-system")
        .expect("Cargo must expose the Pi installation component");
    let output = Command::new(executable)
        .arg("provider-info")
        .output()
        .expect("the built installation component must start");
    assert!(output.status.success(), "provider-info failed");
    assert!(output.stderr.is_empty(), "provider-info wrote stderr");
    let info: Value = serde_json::from_slice(&output.stdout).expect("public provider-info JSON");
    let profiles = info["scoped_projection_profiles"]
        .as_array()
        .expect("scoped profiles must be exposed");
    let projects: Vec<_> = profiles
        .iter()
        .filter(|profile| profile["target_scope"] == "project")
        .collect();
    assert_eq!(
        projects.len(),
        1,
        "Pi must expose one project profile before ai-stp can compile a project bundle"
    );
    let profile = projects[0];
    assert_ne!(
        profile["profile_id"],
        info["projection_profile"]["profile_id"]
    );
    assert_ne!(profile["digest"], info["projection_profile"]["digest"]);
    let kinds = profile["component_kinds"]
        .as_array()
        .expect("component kinds");
    let namespaces = profile["native_namespaces"]
        .as_array()
        .expect("native namespaces");
    for (kind, path) in [
        ("instruction", "AGENTS.md"),
        ("skill", ".pi/skills/review/SKILL.md"),
        ("plugin", ".pi/extensions/review.ts"),
        ("command", ".pi/prompts/review.md"),
        ("setting", ".pi/settings.json"),
    ] {
        assert!(kinds.iter().any(|value| value == kind), "{kind} is absent");
        assert!(owns(namespaces, path), "project route {path} is absent");
    }
    for path in [
        ".pi/auth.json",
        ".pi/mcp-auth.json",
        ".pi/trust.json",
        ".pi/models.json",
        ".pi/sessions/keep.jsonl",
        ".pi/skills-neighbour/keep.md",
        ".agents/skills/someone-else/SKILL.md",
        "auth.json",
        "src/main.rs",
        "AGENTS.override.md",
    ] {
        assert!(!owns(namespaces, path), "project profile claims {path}");
    }
}
