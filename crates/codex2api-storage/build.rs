//! Embed `migrations/NNNN_name.sql` in version order. `0000` is the frozen baseline a
//! fresh database starts from; every later schema change is a new consecutive file, so
//! a skipped or duplicated number fails the build.
use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let directory = manifest.join("migrations");
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut files: Vec<(i64, PathBuf)> = fs::read_dir(&directory)
        .expect("migrations directory")
        .map(|entry| entry.expect("migration entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .map(|path| {
            let name = path.file_name().unwrap().to_str().expect("UTF-8 file name");
            let (prefix, rest) = name
                .split_once('_')
                .filter(|(prefix, _)| prefix.len() == 4)
                .unwrap_or_else(|| panic!("migration {name} must be named NNNN_name.sql"));
            assert!(
                rest.trim_end_matches(".sql")
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "migration {name} must use lowercase snake_case"
            );
            let version: i64 = prefix
                .parse()
                .unwrap_or_else(|_| panic!("migration {name} needs a numeric prefix"));
            println!("cargo:rerun-if-changed={}", path.display());
            (version, path)
        })
        .collect();
    files.sort();
    for (index, (version, path)) in files.iter().enumerate() {
        assert_eq!(
            *version,
            index as i64,
            "migration versions must be consecutive from the 0000 baseline: {}",
            path.display()
        );
    }
    let entries: String = files
        .iter()
        .map(|(version, path)| {
            format!(
                "    ({version}_i64, include_str!({:?})),\n",
                path.to_str().expect("UTF-8 path").replace('\\', "/")
            )
        })
        .collect();
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("migrations.rs");
    fs::write(
        out,
        format!("pub(crate) const MIGRATIONS: &[(i64, &str)] = &[\n{entries}];\n"),
    )
    .expect("write migrations.rs");
}
