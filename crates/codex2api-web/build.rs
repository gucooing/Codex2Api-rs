//! Embed the already-exported Next.js application, refusing stale frontend sources.
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};
fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<String, PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_symlink() {
            panic!("Frontend symlinks are not accepted: {}", path.display());
        }
        if path.is_dir() {
            collect(root, &path, files)
        } else {
            files.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                path,
            );
        }
    }
}
fn main() {
    if env::var_os("CARGO_FEATURE_DEV_FRONTEND").is_some() {
        assert_ne!(
            env::var("PROFILE").as_deref(),
            Ok("release"),
            "dev-frontend is for local development; release builds embed both frontends"
        );
        let output = PathBuf::from(env::var("OUT_DIR").unwrap());
        for name in ["ADMIN_ASSETS", "USER_ASSETS"] {
            fs::write(
                output.join(format!("{name}.rs")),
                format!("pub(crate) static {name}: &[(&str, &[u8])] = &[];\n"),
            )
            .unwrap();
        }
        return;
    }
    embed("frontend", "ADMIN_ASSETS");
    embed("frontend-user", "USER_ASSETS");
}
fn embed(directory: &str, constant: &str) {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../..")
        .join(directory);
    let out = root.join("out");
    let manifest = out.join(".source-manifest.json");
    println!("cargo:rerun-if-changed={}", root.display());
    let raw=fs::read_to_string(&manifest).unwrap_or_else(|_|panic!("Next.js export is missing. Run scripts/build.ps1 or scripts/build.sh (npm ci, test, typecheck, build) before Cargo."));
    let manifest: serde_json::Value =
        serde_json::from_str(&raw).expect("invalid frontend source manifest");
    let recorded = manifest["files"]
        .as_object()
        .expect("frontend manifest must contain files object");
    let mut sources = BTreeMap::new();
    for name in ["src", "app", "components", "lib", "public", "scripts"] {
        let dir = root.join(name);
        if dir.is_dir() {
            collect(&root, &dir, &mut sources);
        }
    }
    for entry in fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let name = path.file_name().unwrap().to_str().unwrap();
            if (name.starts_with("package") && name.ends_with(".json"))
                || name == "components.json"
                || name.starts_with("next.config.")
                || name == "tsconfig.json"
                || name.starts_with("postcss.config.")
                || name.starts_with("eslint.config.")
            {
                sources.insert(name.into(), path);
            }
        }
    }
    if sources.len() != recorded.len() {
        panic!("Frontend source file list changed. Rebuild frontend/out before Cargo.");
    }
    for (name, path) in sources {
        let digest = format!("{:x}", Sha256::digest(fs::read(&path).unwrap()));
        if recorded.get(&name).and_then(|v| v.as_str()) != Some(&digest) {
            panic!("Frontend source changed ({name}). Rebuild frontend/out before Cargo.");
        }
    }
    let mut assets = BTreeMap::new();
    collect(&out, &out, &mut assets);
    assert!(
        assets.contains_key("index.html"),
        "frontend export has no index.html"
    );
    let mut code = format!("pub(crate) static {constant}: &[(&str, &[u8])] = &[\n");
    for (name, path) in assets {
        if name.starts_with('.') {
            continue;
        }
        code.push_str(&format!(
            "({name:?}, include_bytes!({:?})),\n",
            path.canonicalize().unwrap().to_string_lossy()
        ));
    }
    code.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join(format!("{constant}.rs")),
        code,
    )
    .unwrap();
}
