//! Kommandozeilenprogramm `rust2c`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs, process};

const USAGE: &str = "\
Verwendung: rust2c <datei.rs> [Optionen]

Übersetzt eine Teilmenge von Rust nach C.

Optionen:
  -o <datei>     C-Ausgabe in <datei> schreiben (`-` = Standardausgabe).
                 Standard: <datei>.c neben der Quelldatei
  -r, --run      Ergebnis mit dem C-Compiler übersetzen und sofort ausführen
      --cc <cc>  zu verwendender C-Compiler (Standard: $CC oder `cc`)
  -h, --help     diese Hilfe anzeigen
";

struct Options {
    input: PathBuf,
    output: Option<String>,
    run: bool,
    cc: String,
}

fn parse_args() -> Result<Options, String> {
    let mut args = env::args().skip(1);
    let mut input = None;
    let mut output = None;
    let mut run = false;
    let mut cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                process::exit(0);
            }
            "-o" => output = Some(args.next().ok_or("`-o` erwartet einen Dateinamen")?),
            "-r" | "--run" => run = true,
            "--cc" => cc = args.next().ok_or("`--cc` erwartet einen Compiler")?,
            a if a.starts_with('-') => return Err(format!("unbekannte Option `{a}`")),
            a => {
                if input.replace(PathBuf::from(a)).is_some() {
                    return Err("es kann nur eine Quelldatei angegeben werden".into());
                }
            }
        }
    }
    let input = input.ok_or("keine Quelldatei angegeben")?;
    Ok(Options {
        input,
        output,
        run,
        cc,
    })
}

/// Übersetzt die C-Datei mit dem C-Compiler und führt das Programm aus.
fn compile_and_run(cc: &str, c_file: &Path, exe: &Path) -> Result<ExitCode, String> {
    let status = Command::new(cc)
        .args(["-std=c11", "-O2", "-o"])
        .arg(exe)
        .arg(c_file)
        .status()
        .map_err(|e| format!("C-Compiler `{cc}` konnte nicht gestartet werden: {e}"))?;
    if !status.success() {
        return Err(format!("C-Compiler `{cc}` ist fehlgeschlagen"));
    }
    let status = Command::new(exe)
        .status()
        .map_err(|e| format!("Programm konnte nicht gestartet werden: {e}"))?;
    let code = status.code().unwrap_or(1);
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
}

fn run(opts: &Options) -> Result<ExitCode, String> {
    let file_name = opts.input.display().to_string();
    let src = fs::read_to_string(&opts.input)
        .map_err(|e| format!("`{file_name}` kann nicht gelesen werden: {e}"))?;
    let c_code = match rust2c::compile(&src) {
        Ok(c) => c,
        Err(e) => {
            eprint!("{}", e.render(&src, &file_name));
            return Ok(ExitCode::from(1));
        }
    };
    let write = |path: &Path| {
        fs::write(path, &c_code)
            .map_err(|e| format!("`{}` kann nicht geschrieben werden: {e}", path.display()))
    };

    // C-Ausgabe schreiben, falls gewünscht (ohne `--run` immer)
    let written = match opts.output.as_deref() {
        Some("-") => {
            print!("{c_code}");
            None
        }
        Some(o) => {
            write(Path::new(o))?;
            Some(PathBuf::from(o))
        }
        None if !opts.run => {
            let out = opts.input.with_extension("c");
            write(&out)?;
            Some(out)
        }
        None => None,
    };
    if !opts.run {
        return Ok(ExitCode::SUCCESS);
    }

    // Übersetzen und ausführen in einem temporären Verzeichnis
    let dir = env::temp_dir().join(format!("rust2c-{}", process::id()));
    fs::create_dir_all(&dir).map_err(|e| format!("temporäres Verzeichnis: {e}"))?;
    let result = (|| {
        let c_file = match written {
            Some(p) => p,
            None => {
                let p = dir.join("programm.c");
                write(&p)?;
                p
            }
        };
        compile_and_run(&opts.cc, &c_file, &dir.join("programm"))
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("fehler: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&opts) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fehler: {e}");
            ExitCode::from(1)
        }
    }
}
