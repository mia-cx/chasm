//! Compiles generated C to a native executable with the system C compiler.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Feeds `c_source` to `$CC` (default `cc`) on stdin and writes the executable
/// to `output`. The compiler runs directly, never through a shell, so neither
/// `CC` nor the paths are interpreted as shell syntax.
pub fn build(c_source: &str, source: &Path, output: &Path) -> Result<(), String> {
    if same_file(source, output) {
        return Err(format!(
            "refusing to overwrite the source file `{}` with the executable",
            source.display()
        ));
    }
    let cc = env::var_os("CC")
        .filter(|cc| !cc.is_empty())
        .unwrap_or_else(|| OsString::from("cc"));
    let shown = cc.to_string_lossy().into_owned();
    let mut child = Command::new(&cc)
        .args(["-std=c11", "-O2", "-x", "c", "-", "-o"])
        .arg(literal_path(output))
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run the C compiler `{shown}`: {error}"))?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    // A write error means the compiler exited early; its status explains why.
    let written = stdin.write_all(c_source.as_bytes());
    drop(stdin);
    let status = child
        .wait()
        .map_err(|error| format!("cannot wait for the C compiler `{shown}`: {error}"))?;
    if !status.success() {
        return Err(format!("the C compiler `{shown}` failed ({status})"));
    }
    written.map_err(|error| format!("cannot send C code to `{shown}`: {error}"))
}

/// Whether both paths name one existing file, through symlinks or hard links.
fn same_file(a: &Path, b: &Path) -> bool {
    let (Ok(a_meta), Ok(b_meta)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a_meta.dev() == b_meta.dev() && a_meta.ino() == b_meta.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (a_meta, b_meta);
        matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
    }
}

/// Prefixes `./` to a relative path starting with `-`, so the compiler cannot
/// read it as an option.
fn literal_path(path: &Path) -> PathBuf {
    if path.as_os_str().to_string_lossy().starts_with('-') {
        Path::new(".").join(path)
    } else {
        path.to_path_buf()
    }
}
