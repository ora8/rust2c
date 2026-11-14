//! Hilfsfunktionen für die Integrationstests.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Eigenes Arbeitsverzeichnis pro Aufruf.
pub fn work_dir(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("{name}-{}-{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Übersetzt `src` mit rust2c und dem C-Compiler, führt das Programm aus.
pub fn compile_and_run(name: &str, src: &str) -> Output {
    let c = rust2c::compile(src).unwrap_or_else(|e| panic!("{}", e.render(src, name)));
    let dir = work_dir(name);
    let c_file = dir.join("prog.c");
    let exe = dir.join("prog");
    fs::write(&c_file, c).unwrap();
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    let out = Command::new(&cc)
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-o"])
        .arg(&exe)
        .arg(&c_file)
        .output()
        .expect("C-Compiler nicht gefunden");
    assert!(
        out.status.success(),
        "C-Compiler meldet Fehler für {name}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Command::new(&exe).output().unwrap()
}
