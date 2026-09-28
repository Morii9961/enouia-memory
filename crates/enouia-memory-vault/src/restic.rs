//! restic adapter for encrypted backups of a pinned-commit export
//! (PRIVACY_RECOVERY §4). The project does not implement its own backup
//! cryptography; restic encrypts. This module only builds the commands,
//! checks the pinned version, and parses restic's JSON summary.
//!
//! Status (MV-1.4): restic is not installed on the development machine, so
//! no live backup, check, or restore has run. The owner chooses the backup
//! media and keeps the repository password outside this device; the password
//! reaches restic only through the child process environment, never through
//! arguments, logs, the Vault, or an export.

use enouia_memory_contract::ports::SecretBytes;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct ResticConfig {
    /// Absolute path of the verified restic executable.
    pub binary: PathBuf,
    /// Exact version the owner installed and verified (e.g. "0.18.0").
    pub expected_version: String,
    /// Repository location chosen by the owner (never a path in this repository).
    pub repository: String,
}

/// Why a restic step cannot proceed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResticError {
    VersionMismatch,
    PasswordNotUtf8,
    NoSnapshotInOutput,
}

pub struct Restic {
    config: ResticConfig,
}

impl Restic {
    pub fn new(config: ResticConfig) -> Self {
        Self { config }
    }

    fn base(&self, password: &SecretBytes) -> Result<Command, ResticError> {
        let password =
            std::str::from_utf8(password.expose()).map_err(|_| ResticError::PasswordNotUtf8)?;
        let mut command = Command::new(&self.config.binary);
        command
            .env_clear()
            .env("RESTIC_PASSWORD", password)
            .env("RESTIC_REPOSITORY", &self.config.repository)
            .arg("--no-cache");
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        Ok(command)
    }

    pub fn version_command(&self) -> Command {
        let mut command = Command::new(&self.config.binary);
        command.arg("version");
        command
    }

    /// `restic version` prints `restic 0.18.0 compiled with go…`.
    pub fn version_matches(&self, output: &str) -> bool {
        output
            .split_whitespace()
            .nth(1)
            .is_some_and(|v| v == self.config.expected_version)
    }

    /// Back up one export directory, stored with relative paths and tagged
    /// with the commit it pins.
    pub fn backup_command(
        &self,
        password: &SecretBytes,
        export_dir: &Path,
        commit_id: &str,
    ) -> Result<Command, ResticError> {
        let mut command = self.base(password)?;
        command
            .current_dir(export_dir)
            .args(["backup", "--json", "--tag", "enouia-memory", "--tag"])
            .arg(commit_id)
            .arg(".");
        Ok(command)
    }

    /// Verify repository structure and a sample of pack data.
    pub fn check_command(&self, password: &SecretBytes) -> Result<Command, ResticError> {
        let mut command = self.base(password)?;
        command.args(["check", "--read-data-subset=10%"]);
        Ok(command)
    }

    /// Restore a snapshot into an empty directory for `backup::verify_export`
    /// and `backup::restore_export` (never directly into a live Vault).
    pub fn restore_command(
        &self,
        password: &SecretBytes,
        snapshot_id: &str,
        target: &Path,
    ) -> Result<Command, ResticError> {
        let mut command = self.base(password)?;
        command
            .args(["restore", snapshot_id, "--target"])
            .arg(target);
        Ok(command)
    }

    /// Snapshot ID from `restic backup --json` output (the summary line).
    pub fn snapshot_id(output: &str) -> Result<String, ResticError> {
        output
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|v| v["message_type"] == "summary")
            .find_map(|v| v["snapshot_id"].as_str().map(str::to_owned))
            .ok_or(ResticError::NoSnapshotInOutput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restic() -> Restic {
        Restic::new(ResticConfig {
            binary: PathBuf::from("restic.exe"),
            expected_version: "0.18.0".to_owned(),
            repository: "local-test-repository".to_owned(),
        })
    }

    #[test]
    fn the_password_travels_only_in_the_environment() {
        let secret = SecretBytes::new(b"sentinel-restic-password".to_vec());
        let command = restic()
            .backup_command(&secret, Path::new("."), "cmt_1")
            .unwrap();
        let args: Vec<String> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().all(|a| !a.contains("sentinel")));
        assert_eq!(args.last().map(String::as_str), Some("."));
        let env: Vec<_> = command.get_envs().collect();
        assert!(
            env.iter()
                .any(|(k, v)| k.to_str() == Some("RESTIC_PASSWORD")
                    && v.is_some_and(|v| v.to_str() == Some("sentinel-restic-password")))
        );
        assert!(
            env.iter().all(|(k, _)| k.to_str() != Some("PATH")),
            "environment is cleared, not inherited"
        );
    }

    #[test]
    fn version_and_summary_parsing() {
        let r = restic();
        assert!(r.version_matches("restic 0.18.0 compiled with go1.24 on windows/amd64"));
        assert!(!r.version_matches("restic 0.17.3 compiled with go1.23 on windows/amd64"));
        let out = "{\"message_type\":\"status\",\"percent_done\":0.5}\n{\"message_type\":\"summary\",\"snapshot_id\":\"4f2a\"}\n";
        assert_eq!(Restic::snapshot_id(out).unwrap(), "4f2a");
        assert_eq!(
            Restic::snapshot_id("{\"message_type\":\"status\"}"),
            Err(ResticError::NoSnapshotInOutput)
        );
        assert_eq!(
            restic().check_command(&SecretBytes::new(vec![0xff])).err(),
            Some(ResticError::PasswordNotUtf8)
        );
    }
}
