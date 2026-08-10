//! Resolution of a provider `command` to the file that is actually launched.
//!
//! On Unix the bare name goes to `execvp`, which already searches `PATH`. On
//! Windows `CreateProcess` only ever appends `.exe`, so a CLI shipped as a batch
//! wrapper is invisible to `Command::new`: the Azure CLI installs `az.cmd` and
//! never an `az.exe`, and `command: az` fails with "program not found" before
//! any policy decision is even reached. This module walks `PATH` × `PATHEXT` the
//! way `cmd.exe` does and hands `Command::new` an absolute path.

use crate::error::Result;
use std::path::PathBuf;

#[cfg(not(windows))]
pub fn resolve(program: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(program))
}

/// Extensions Windows treats as executable when `PATHEXT` is absent or empty.
#[cfg(windows)]
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// Resolve `program` against `PATH` and `PATHEXT`, returning an absolute path.
///
/// The search covers `PATH` only. `CreateProcess` would also probe the current
/// directory and the directory of the running executable; an agent able to drop
/// a file into either could otherwise shadow a provider CLI, so those are left
/// out deliberately. A resolved absolute path also stops the child from
/// repeating the lookup under a different `PATH`.
///
/// The extension is spelled as `PATHEXT` spells it, so the returned path may
/// differ in case from the directory entry it names. Windows resolves either
/// spelling to the same file; treat the result as a path to launch, not as the
/// file's canonical name.
#[cfg(windows)]
pub fn resolve(program: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH");
    let pathext = std::env::var_os("PATHEXT");
    resolve_in(program, path.as_deref(), pathext.as_deref())
}

#[cfg(windows)]
fn resolve_in(
    program: &str,
    path: Option<&std::ffi::OsStr>,
    pathext: Option<&std::ffi::OsStr>,
) -> Result<PathBuf> {
    use std::path::Path;

    let extensions = executable_extensions(pathext);
    // A command carrying any path information is resolved relative to itself,
    // exactly like `cmd.exe`; only a bare name searches `PATH`.
    if program.is_empty() {
        return Err(missing(program));
    }
    if program.contains([':', '/', '\\']) {
        return first_existing(Path::new(program), &extensions).ok_or_else(|| missing(program));
    }
    let path = path.ok_or_else(|| missing(program))?;
    // Directory by directory, each one trying every extension in order — the
    // search order of `cmd.exe`, so a `.cmd` earlier in `PATH` wins over an
    // `.exe` later in it.
    std::env::split_paths(path)
        .filter(|directory| !directory.as_os_str().is_empty())
        .find_map(|directory| first_existing(&directory.join(program), &extensions))
        .ok_or_else(|| missing(program))
}

#[cfg(windows)]
fn missing(program: &str) -> crate::error::Error {
    crate::error::Error::ProgramNotFound {
        program: program.to_string(),
    }
}

/// First launchable file for `base`, appending `extensions` when needed.
///
/// A name that already carries an executable extension is taken as final: an
/// `az.cmd` that does not exist must not silently become `az.cmd.exe`.
#[cfg(windows)]
fn first_existing(base: &std::path::Path, extensions: &[String]) -> Option<PathBuf> {
    if has_executable_extension(base, extensions) {
        return is_file(base).then(|| base.to_path_buf());
    }
    extensions
        .iter()
        .map(|extension| append_extension(base, extension))
        .find(|candidate| is_file(candidate))
}

#[cfg(windows)]
fn append_extension(base: &std::path::Path, extension: &str) -> PathBuf {
    let mut value = base.as_os_str().to_os_string();
    value.push(extension);
    PathBuf::from(value)
}

#[cfg(windows)]
fn has_executable_extension(base: &std::path::Path, extensions: &[String]) -> bool {
    let Some(extension) = base.extension().and_then(std::ffi::OsStr::to_str) else {
        return false;
    };
    extensions.iter().any(|candidate| {
        candidate
            .strip_prefix('.')
            .unwrap_or(candidate)
            .eq_ignore_ascii_case(extension)
    })
}

#[cfg(windows)]
fn is_file(candidate: &std::path::Path) -> bool {
    candidate.metadata().is_ok_and(|data| data.is_file())
}

/// `PATHEXT` split into normalized extensions, falling back to the Windows set.
#[cfg(windows)]
fn executable_extensions(pathext: Option<&std::ffi::OsStr>) -> Vec<String> {
    let declared = pathext
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|extension| !extension.is_empty() && *extension != ".")
        .map(|extension| {
            if extension.starts_with('.') {
                extension.to_string()
            } else {
                format!(".{extension}")
            }
        })
        .collect::<Vec<_>>();
    if declared.is_empty() {
        return DEFAULT_PATHEXT
            .split(';')
            .map(str::to_string)
            .collect::<Vec<_>>();
    }
    declared
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::path::Path;

    fn touch(directory: &Path, name: &str) {
        fs::write(directory.join(name), b"").unwrap();
    }

    fn path_variable(directories: &[&Path]) -> OsString {
        std::env::join_paths(directories.iter().map(|d| d.as_os_str())).unwrap()
    }

    /// Compare by identity, not by spelling: the resolved extension carries the
    /// case of `PATHEXT`, which need not match the directory entry.
    fn assert_resolved_to(resolved: PathBuf, expected: PathBuf) {
        assert_eq!(
            fs::canonicalize(&resolved).unwrap(),
            fs::canonicalize(&expected).unwrap(),
            "{} should name the same file as {}",
            resolved.display(),
            expected.display()
        );
    }

    #[test]
    fn a_batch_wrapper_is_found_when_no_executable_exists() {
        let temp = tempfile::TempDir::new().unwrap();
        touch(temp.path(), "az.cmd");
        let path = path_variable(&[temp.path()]);
        assert_resolved_to(
            resolve_in("az", Some(&path), None).unwrap(),
            temp.path().join("az.cmd"),
        );
    }

    #[test]
    fn pathext_order_decides_between_extensions_in_one_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        touch(temp.path(), "tool.cmd");
        touch(temp.path(), "tool.exe");
        let path = path_variable(&[temp.path()]);
        assert_resolved_to(
            resolve_in("tool", Some(&path), None).unwrap(),
            temp.path().join("tool.exe"),
        );
        assert_resolved_to(
            resolve_in("tool", Some(&path), Some(&OsString::from(".CMD;.EXE"))).unwrap(),
            temp.path().join("tool.cmd"),
        );
    }

    #[test]
    fn an_earlier_directory_wins_over_an_earlier_extension() {
        let temp = tempfile::TempDir::new().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        touch(&first, "tool.cmd");
        touch(&second, "tool.exe");
        let path = path_variable(&[&first, &second]);
        assert_resolved_to(
            resolve_in("tool", Some(&path), None).unwrap(),
            first.join("tool.cmd"),
        );
    }

    #[test]
    fn a_directory_named_like_the_program_is_not_launchable() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("tool.exe")).unwrap();
        touch(temp.path(), "tool.cmd");
        let path = path_variable(&[temp.path()]);
        assert_resolved_to(
            resolve_in("tool", Some(&path), None).unwrap(),
            temp.path().join("tool.cmd"),
        );
    }

    #[test]
    fn an_explicit_executable_extension_is_taken_as_final() {
        let temp = tempfile::TempDir::new().unwrap();
        touch(temp.path(), "tool.cmd");
        let path = path_variable(&[temp.path()]);
        assert_resolved_to(
            resolve_in("tool.cmd", Some(&path), None).unwrap(),
            temp.path().join("tool.cmd"),
        );
        assert!(resolve_in("tool.exe", Some(&path), None).is_err());
    }

    #[test]
    fn a_command_with_path_information_does_not_search_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let elsewhere = temp.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        touch(&elsewhere, "tool.cmd");
        let path = path_variable(&[temp.path()]);
        let absolute = elsewhere.join("tool");
        assert_resolved_to(
            resolve_in(absolute.to_str().unwrap(), Some(&path), None).unwrap(),
            elsewhere.join("tool.cmd"),
        );
    }

    #[test]
    fn a_directory_outside_path_is_not_searched() {
        let temp = tempfile::TempDir::new().unwrap();
        touch(temp.path(), "tool.cmd");
        let empty = temp.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        let path = path_variable(&[&empty]);
        let error = resolve_in("tool", Some(&path), None).unwrap_err();
        assert!(error.to_string().contains("tool"));
    }

    #[test]
    fn a_missing_program_names_itself_in_the_error() {
        let error = resolve_in("executable-that-must-not-run", None, None).unwrap_err();
        assert!(error.to_string().contains("executable-that-must-not-run"));
    }

    #[test]
    fn an_empty_pathext_falls_back_to_the_windows_default() {
        assert_eq!(
            executable_extensions(Some(&OsString::from(" ; ; "))),
            vec![".COM", ".EXE", ".BAT", ".CMD"]
        );
        assert_eq!(
            executable_extensions(Some(&OsString::from("EXE;.cmd"))),
            vec![".EXE", ".cmd"]
        );
    }
}
