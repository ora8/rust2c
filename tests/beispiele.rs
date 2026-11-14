//! Übersetzt alle Programme in `beispiele/` und vergleicht die Ausgabe
//! mit `*.erwartet` – und, falls vorhanden, mit dem echten `rustc`.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn beispiele() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("beispiele");
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    files
}

fn name(p: &Path) -> String {
    p.file_stem().unwrap().to_string_lossy().into_owned()
}

#[test]
fn ausgabe_wie_erwartet() {
    for file in beispiele() {
        let src = fs::read_to_string(&file).unwrap();
        let out = common::compile_and_run(&name(&file), &src);
        assert!(
            out.status.success(),
            "{} endete mit {:?}",
            file.display(),
            out.status
        );
        let expected = fs::read_to_string(file.with_extension("erwartet"))
            .unwrap_or_else(|_| panic!("{}.erwartet fehlt", name(&file)));
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            expected,
            "{}",
            file.display()
        );
    }
}

/// Die Beispiele sind gültiges Rust: rustc muss dasselbe ausgeben.
#[test]
fn gleich_wie_rustc() {
    if Command::new("rustc").arg("--version").output().is_err() {
        eprintln!("rustc nicht gefunden – Test übersprungen");
        return;
    }
    for file in beispiele() {
        let n = name(&file);
        let dir = common::work_dir(&format!("rustc-{n}"));
        let exe = dir.join(&n);
        let status = Command::new("rustc")
            .args(["--edition", "2021", "-A", "warnings", "-o"])
            .arg(&exe)
            .arg(&file)
            .status()
            .unwrap();
        assert!(status.success(), "rustc lehnt {} ab", file.display());
        let want = Command::new(&exe).output().unwrap();
        let src = fs::read_to_string(&file).unwrap();
        let got = common::compile_and_run(&n, &src);
        assert_eq!(
            String::from_utf8_lossy(&got.stdout),
            String::from_utf8_lossy(&want.stdout),
            "{}",
            file.display()
        );
    }
}
