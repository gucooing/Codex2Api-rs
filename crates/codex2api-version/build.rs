use std::{env, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn main() {
    println!("cargo:rerun-if-env-changed=CODEX2API_BUILD_TAG");
    println!("cargo:rerun-if-changed=build.rs");
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest directory");
    let root = Path::new(&manifest).join("../..");

    // Track tag and checkout changes, including linked worktrees and packed refs.
    for name in ["HEAD", "packed-refs", "refs/tags"] {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }
    if let Some(head) = git(&root, &["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&root, &["rev-parse", "--git-path", &head])
    {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }

    // Release builds inject their exact tag; local builds use the latest reachable tag.
    let version = env::var("CODEX2API_BUILD_TAG")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| git(&root, &["describe", "--tags", "--abbrev=0"]))
        .unwrap_or_else(|| "unknown".to_owned());
    assert!(
        !version.contains(['\r', '\n']),
        "build tag must be a single line"
    );
    println!("cargo:rustc-env=CODEX2API_BUILD_TAG={version}");
}
