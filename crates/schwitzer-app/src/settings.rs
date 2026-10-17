//! Persistent settings and filesystem locations.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The port of the MCP server unless `settings.toml` says otherwise.
pub const DEFAULT_MCP_PORT: u16 = 13339;

/// File name of the settings inside the data directory.
const FILE: &str = "settings.toml";

/// The contents of `settings.toml`. Missing keys take their defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The display name shown to the other members.
    pub name: String,
    /// The port the MCP server listens on, on `127.0.0.1`.
    pub mcp_port: u16,
    /// Where omp keeps its session files.
    pub omp_sessions_dir: PathBuf,
    /// The server of the last created room; prefills "Create room".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_server: Option<String>,
    /// The room keys, in the order the rooms are listed.
    pub rooms: Vec<String>,
    /// Per room (id in hex), the local repositories whose omp sessions are shared with it.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub repos: BTreeMap<String, Vec<PathBuf>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: default_name(),
            mcp_port: DEFAULT_MCP_PORT,
            omp_sessions_dir: default_sessions_dir(),
            last_server: None,
            rooms: Vec::new(),
            repos: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Reads `settings.toml` from `dir`. A missing file gives the defaults; a file that cannot
    /// be read or parsed gives the defaults and the reason. The file is left alone: only
    /// [`Settings::save`] writes it.
    pub fn load(dir: &Path) -> (Self, Option<String>) {
        let path = dir.join(FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(settings) => (settings, None),
                Err(e) => (Self::default(), Some(format!("{}: {e}", path.display()))),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => (Self::default(), None),
            Err(e) => (Self::default(), Some(format!("{}: {e}", path.display()))),
        }
    }

    /// Writes `settings.toml` into `dir` through a temporary file and a rename.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, dir.join(FILE))
    }
}

/// `$SCHWITZER_HOME`, else the platform's data directory for the app.
pub fn data_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("SCHWITZER_HOME").filter(|h| !h.is_empty()) {
        return PathBuf::from(home);
    }
    directories::ProjectDirs::from("", "", "schwitzer").map_or_else(
        || PathBuf::from(".schwitzer"),
        |dirs| dirs.data_dir().to_path_buf(),
    )
}

/// The weida identity (key and certificate) lives under the data directory.
pub fn identity_dir(data: &Path) -> PathBuf {
    data.join("identity")
}

fn default_name() -> String {
    ["USER", "USERNAME"]
        .into_iter()
        .filter_map(|var| std::env::var(var).ok())
        .map(|name| name.trim().to_owned())
        .find(|name| !name.is_empty())
        .unwrap_or_else(|| "anon".to_owned())
}

fn default_sessions_dir() -> PathBuf {
    directories::BaseDirs::new().map_or_else(
        || PathBuf::from(".omp/agent/sessions"),
        |dirs| dirs.home_dir().join(".omp/agent/sessions"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults_without_a_problem() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, problem) = Settings::load(dir.path());
        assert_eq!(settings, Settings::default());
        assert_eq!(problem, None);
        assert!(!dir.path().join(FILE).exists());
    }

    #[test]
    fn unparsable_file_is_reported_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "mcp_port = \"nope\"").unwrap();
        let (settings, problem) = Settings::load(dir.path());
        assert_eq!(settings, Settings::default());
        assert!(problem.is_some_and(|p| p.contains(FILE)));
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE)).unwrap(),
            "mcp_port = \"nope\""
        );
    }

    #[test]
    fn partial_file_keeps_the_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(FILE),
            "name = \"alice\"\nmcp_port = 13401\n",
        )
        .unwrap();
        let (settings, problem) = Settings::load(dir.path());
        assert_eq!(problem, None);
        assert_eq!(settings.name, "alice");
        assert_eq!(settings.mcp_port, 13401);
        assert_eq!(settings.omp_sessions_dir, default_sessions_dir());
    }

    #[test]
    fn save_roundtrips_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            name: "bob".to_owned(),
            mcp_port: 1,
            omp_sessions_dir: PathBuf::from("/tmp/omp"),
            last_server: Some("weida://sha256:00@host:7444".to_owned()),
            rooms: vec!["schwitzer://a".to_owned(), "schwitzer://b".to_owned()],
            repos: BTreeMap::from([("ab".to_owned(), vec![PathBuf::from("/code/x")])]),
        };
        settings.save(dir.path()).unwrap();
        assert!(!dir.path().join("settings.toml.tmp").exists());
        assert_eq!(Settings::load(dir.path()), (settings, None));
    }
}
