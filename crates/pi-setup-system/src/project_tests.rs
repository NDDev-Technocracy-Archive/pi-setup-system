//! Project-scoped wire operations over disposable directories on every OS.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use provider_v3::{
    TargetScope, WireReason, argv,
    bundle::{
        BUNDLE_DOMAIN, BUNDLE_FORMAT, BUNDLE_PROTOCOL_VERSION, FILES_PREFIX, MANIFEST_MEMBER,
        REQUIRED_MEMBERS,
    },
    zip::build::{Entry, write},
};
use serde_json::{Value, json};

use super::PI;

const RELEASE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CANARY: &str = "UNOWNED_PI_PROJECT_CANARY";
const PRESERVED: &[&str] = &[
    ".pi/auth.json",
    ".pi/mcp-auth.json",
    ".pi/trust.json",
    ".pi/models.json",
    ".pi/sessions/session.jsonl",
    ".pi/SYSTEM.md",
    ".pi/themes/personal.json",
    ".pi/skills/foreign/SKILL.md",
    ".pi/extensions/foreign.ts",
    ".pi/prompts/foreign.md",
    ".pi/skills-neighbour/keep.md",
    ".agents/skills/foreign/SKILL.md",
    "settings.json",
    "src/main.rs",
    ".git/config",
];

struct Fixture {
    root: PathBuf,
    target: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("pi-project-{label}-{}-{nonce}", std::process::id()));
        let target = root.join("checkout with spaces");
        fs::create_dir_all(&target).unwrap();
        let root = root.canonicalize().unwrap();
        let target = target.canonicalize().unwrap();
        for path in PRESERVED {
            let path = target.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, CANARY).unwrap();
        }
        Self { root, target }
    }

    fn check_preserved(&self) {
        for path in PRESERVED {
            assert_eq!(
                fs::read(self.target.join(path)).unwrap(),
                CANARY.as_bytes(),
                "changed {path}"
            );
        }
        let control = self.target.join(PI.control_directory);
        if control.exists() {
            no_captured_canary(&control);
        }
    }

    fn args(&self, command: &str, extra: Vec<String>) -> Vec<String> {
        let mut args = vec![
            command.into(),
            "--target".into(),
            self.target.to_str().unwrap().into(),
            "--json".into(),
        ];
        args.extend(extra);
        args
    }

    fn call(&self, command: &str, extra: Vec<String>) -> provider_v3::Result<Value> {
        harness_runtime::dispatch(&PI, argv::parse(self.args(command, extra))?)
    }

    fn plan(&self, operation: &str, id: &str, bundle: Vec<String>) -> Value {
        let mut args = strings(&[
            "--operation",
            operation,
            "--operation-id",
            id,
            "--provider-release-digest",
            RELEASE,
            "--expires-at",
            "2099-01-01T00:00:00.000Z",
            "--recovery-mode",
            "forward",
            "--target-scope",
            "project",
        ]);
        args.extend(bundle);
        self.call("plan-operation", args).unwrap()
    }

    fn apply_args(&self, id: &str, plan: &Value, bundle: Vec<String>) -> Vec<String> {
        let path = self.root.join(format!("{id}.json"));
        fs::write(
            &path,
            setup_core::canonical::to_canonical_bytes(&plan["plan"]).unwrap(),
        )
        .unwrap();
        let mut args = strings(&[
            "--plan",
            path.to_str().unwrap(),
            "--plan-digest",
            plan["plan_digest"].as_str().unwrap(),
            "--provider-release-digest",
            RELEASE,
        ]);
        args.extend(bundle);
        args
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

fn no_captured_canary(path: &Path) {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let metadata = entry.file_type().unwrap();
        assert!(!metadata.is_symlink());
        if metadata.is_dir() {
            no_captured_canary(&entry.path());
        } else if metadata.is_file() {
            let bytes = fs::read(entry.path()).unwrap();
            assert!(
                !bytes
                    .windows(CANARY.len())
                    .any(|window| window == CANARY.as_bytes()),
                "provider control state captured an unowned canary"
            );
        }
    }
}

type File<'a> = (&'a str, &'a str, &'a str);

fn desired(next: bool) -> Vec<File<'static>> {
    vec![
        (
            "plugin",
            ".pi/extensions/review.ts",
            if next {
                "export default function next() {}\n"
            } else {
                "export default function first() {}\n"
            },
        ),
        (
            "command",
            ".pi/prompts/review.md",
            if next {
                "Review the updated change.\n"
            } else {
                "Review this change.\n"
            },
        ),
        (
            "setting",
            ".pi/settings.json",
            if next {
                "{\"quietStartup\":false}\n"
            } else {
                "{\"quietStartup\":true}\n"
            },
        ),
        (
            "skill",
            ".pi/skills/review/SKILL.md",
            if next {
                "---\nname: review\ndescription: Updated review\n---\nReview.\n"
            } else {
                "---\nname: review\ndescription: Review\n---\nReview.\n"
            },
        ),
        (
            "instruction",
            "AGENTS.md",
            if next {
                "# Updated review\n"
            } else {
                "# Review\n"
            },
        ),
    ]
}

fn manifest(files: &[File<'_>]) -> Value {
    let profile = PI
        .projection_profile_for(Some(TargetScope::Project))
        .unwrap();
    let records: Vec<_> = files.iter().enumerate().map(|(index, (_, path, body))| json!({
        "schema_version":1, "path":path, "digest":setup_core::digest::of_bytes(body.as_bytes()),
        "byte_length":body.len(), "mode":0o644, "owner":format!("component_{:026}", index+1),
    })).collect();
    let adaptations: Vec<_> = files.iter().enumerate().map(|(index, (kind, path, _))| json!({
        "stable_id":format!("component_{:026}", index+1), "version":"1.0",
        "passport_digest":format!("sha256:{}", "1".repeat(64)),
        "adaptation_id":format!("adaptation_{:064x}", index+1),
        "projection_artifact":{"digest":format!("sha256:{}", "3".repeat(64)),"size_bytes":128},
        "provider_component_kind":kind,"projection_kind":"native_files","member_paths":[path],
    })).collect();
    let entries: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(index, (kind, path, _))| {
            json!({
                "stable_id":format!("component_{:026}",index+1),"component_type":kind,
                "native_surface":path,"state":"complete","losses":[],
            })
        })
        .collect();
    json!({"schema_version":1,"bundle_format":BUNDLE_FORMAT,"protocol_version":BUNDLE_PROTOCOL_VERSION,
        "harness_id":PI.harness_id,"builder_version":"0.1.0","target_scope":"project",
        "input_digest":format!("sha256:{}","3".repeat(64)),
        "projection_profile":{"profile_id":profile.profile_id,"profile_digest":profile.digest,"target_scope":"project"},
        "component_adaptations":adaptations,"managed_paths":files.iter().map(|(_,path,_)|*path).collect::<Vec<_>>(),
        "files":records,"limits":{"max_files":2000,"max_file_bytes":4*1024*1024,"max_bundle_bytes":64*1024*1024},
        "conversion_report":{"complete":true,"entries":entries}})
}

fn bundle(root: &Path, id: &str, files: &[File<'_>]) -> (PathBuf, Vec<String>) {
    let mut manifest = manifest(files);
    let digest = setup_core::digest::of_domain_canonical_json(BUNDLE_DOMAIN, &manifest).unwrap();
    manifest["bundle_digest"] = digest.clone().into();
    let mut entries = vec![Entry {
        name: MANIFEST_MEMBER.into(),
        data: setup_core::canonical::to_canonical_bytes(&manifest).unwrap(),
        mode: 0o644,
    }];
    for name in REQUIRED_MEMBERS.iter().skip(1) {
        let data = if *name == "setup-passport.json" {
            serde_json::to_vec(&json!({"stable_id":"setup_00000000000000000000000000","version":"3.1.0","harness_id":PI.harness_id})).unwrap()
        } else {
            b"{}".to_vec()
        };
        entries.push(Entry {
            name: (*name).into(),
            data,
            mode: 0o644,
        });
    }
    for (_, path, body) in files {
        entries.push(Entry {
            name: format!("{FILES_PREFIX}{path}"),
            data: body.as_bytes().to_vec(),
            mode: 0o644,
        });
    }
    let bytes = write(&entries);
    let artifact = setup_core::digest::of_bytes(&bytes);
    let path = root.join(format!("{id}.zip"));
    fs::write(&path, &bytes).unwrap();
    let flags = strings(&[
        "--bundle",
        path.to_str().unwrap(),
        "--bundle-format",
        BUNDLE_FORMAT,
        "--bundle-digest",
        &digest,
        "--artifact-digest",
        &artifact,
        "--bundle-size",
        &bytes.len().to_string(),
    ]);
    (path, flags)
}

fn check_desired(fixture: &Fixture, files: &[File<'_>]) {
    for (_, path, bytes) in files {
        assert_eq!(
            fs::read(fixture.target.join(path)).unwrap(),
            bytes.as_bytes(),
            "wrong bytes at {path}"
        );
    }
    fixture.check_preserved();
}

#[test]
fn project_wire_lifecycle_preserves_siblings_and_resumes_prepared_bytes_without_source() {
    let fixture = Fixture::new("lifecycle");
    let first = desired(false);
    let (archive, flags) = bundle(&fixture.root, "install", &first);
    let planned = fixture.plan("install", "operation_PI_PROJECT_INSTALL", flags.clone());
    assert_eq!(planned["state"], "planned");
    let apply = fixture.apply_args("install", &planned, flags);
    let mut prepare = apply.clone();
    prepare.push("--prepare".into());
    assert_eq!(
        fixture.call("apply-operation", prepare).unwrap()["state"],
        "prepared"
    );
    for (_, path, _) in &first {
        assert!(!fixture.target.join(path).exists());
    }
    fixture.check_preserved();
    fs::remove_file(archive).unwrap();
    let installed = fixture.call("apply-operation", apply.clone()).unwrap();
    assert_eq!(installed["state"], "verified");
    assert!(installed["backup_ref"].is_null());
    check_desired(&fixture, &first);
    assert_eq!(fixture.call("apply-operation", apply).unwrap(), installed);
    fixture.call("recover-operation", vec![]).unwrap();
    check_desired(&fixture, &first);

    let second = desired(true);
    let (_, flags) = bundle(&fixture.root, "replace", &second);
    let plan = fixture.plan("replace", "operation_PI_PROJECT_REPLACE", flags.clone());
    let apply = fixture.apply_args("replace", &plan, flags);
    assert_eq!(
        fixture.call("apply-operation", apply).unwrap()["state"],
        "verified"
    );
    check_desired(&fixture, &second);

    let plan = fixture.plan("remove", "operation_PI_PROJECT_REMOVE", vec![]);
    let apply = fixture.apply_args("remove", &plan, vec![]);
    assert_eq!(
        fixture.call("apply-operation", apply.clone()).unwrap()["state"],
        "verified"
    );
    fixture.call("apply-operation", apply).unwrap();
    for (_, path, _) in &second {
        assert!(!fixture.target.join(path).exists());
    }
    fixture.check_preserved();
}

#[test]
fn project_plan_refuses_global_settings_and_unowned_project_paths_before_writes() {
    let fixture = Fixture::new("refusals");
    for (index, path) in [
        "settings.json",
        ".pi/auth.json",
        ".pi/trust.json",
        ".pi/mcp.json",
        ".pi/SYSTEM.md",
        ".pi/skills-neighbour/review.md",
        "src/main.rs",
    ]
    .iter()
    .enumerate()
    {
        let (_, flags) = bundle(
            &fixture.root,
            &format!("bad-{index}"),
            &[("setting", path, "{}\n")],
        );
        let mut args = strings(&[
            "--operation",
            "install",
            "--operation-id",
            &format!("operation_PI_BAD_{index}"),
            "--provider-release-digest",
            RELEASE,
            "--expires-at",
            "2099-01-01T00:00:00.000Z",
            "--recovery-mode",
            "forward",
            "--target-scope",
            "project",
        ]);
        args.extend(flags);
        let refusal = fixture.call("plan-operation", args).unwrap_err();
        assert_eq!(
            refusal.reason(),
            Some(WireReason::UnsupportedNativeSurface),
            "{path}: {refusal}"
        );
        fixture.check_preserved();
    }
}
