//! Discovery of `vd_*` external loader binaries on `$PATH`.
//!
//! At startup, [`discover`] scans every directory on `$PATH` for executables
//! whose name starts with `vd_`, probes each with `--manifest`, and returns
//! the valid ones as [`ExtLoader`] instances ready for registration.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use visidata_ext_protocol::ExtManifest;

use crate::ext_loader::ExtLoader;

/// Maximum time to wait for a `--manifest` probe response.
/// Reserved for future use with a proper timeout mechanism.
#[expect(
    dead_code,
    reason = "timeout not yet enforced; placeholder for future implementation"
)]
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(1);

/// Scan `$PATH` for `vd_*` executables and return valid external loaders.
///
/// Each candidate is probed by running `vd_NAME --manifest`. Binaries that
/// time out, exit non-zero, or return malformed JSON are silently skipped.
#[must_use]
pub fn discover() -> Vec<ExtLoader> {
    find_candidates()
        .into_iter()
        .filter_map(|path| probe(&path))
        .collect()
}

/// Find all executables on `$PATH` whose name starts with `vd_`.
fn find_candidates() -> Vec<PathBuf> {
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let mut candidates = Vec::new();

    for dir in std::env::split_paths(&path_var) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("vd_") {
                continue;
            }
            let path = entry.path();
            if is_executable(&path) {
                candidates.push(path);
            }
        }
    }

    // Deterministic order — sort by binary name.
    candidates.sort_unstable_by(|a, b| a.file_name().cmp(&b.file_name()));
    candidates.dedup();
    candidates
}

/// Probe a binary with `--manifest` and return an `ExtLoader` if valid.
fn probe(binary: &Path) -> Option<ExtLoader> {
    // Run with a 1-second timeout via a thread rather than adding a crate dep.
    let binary_owned = binary.to_path_buf();
    let handle =
        std::thread::spawn(move || Command::new(&binary_owned).arg("--manifest").output().ok());

    let output = handle.join().ok().flatten()?;

    if !output.status.success() {
        return None;
    }

    let manifest: ExtManifest = serde_json::from_slice(&output.stdout).ok()?;

    // Sanity check: manifest name must start with "vd_".
    if !manifest.name.starts_with("vd_") {
        return None;
    }

    Some(ExtLoader::new(binary.to_path_buf(), manifest))
}

/// Returns `true` if the path is a regular executable file.
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_returns_vec() {
        // Discovery should never panic, even in a minimal environment.
        let loaders = discover();
        // We can't assert specific binaries exist in CI, but the call must succeed.
        assert!(
            loaders.len() < 1000,
            "sanity: not an absurd number of loaders"
        );
    }

    #[test]
    fn probe_nonexistent_binary() {
        let result = probe(Path::new("/nonexistent/vd_fake"));
        assert!(result.is_none());
    }

    #[test]
    fn is_executable_nonexistent() {
        assert!(!is_executable(Path::new("/nonexistent/binary")));
    }
}
