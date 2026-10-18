//! The omp plugin in shared repos.
//!
//! The app writes the plugin (`omp-plugin/index.ts`, a `package.json` and `schwitzer.json`)
//! into `<repo>/.omp/plugins/node_modules/schwitzer/` and enters it in
//! `<repo>/.omp/plugins/omp-plugins.lock.json`, so omp started in the repo loads it as a
//! project-scope plugin. It gives omp `/schwitzer`, which plans a task of the rooms the repo
//! is shared with through the app's MCP server.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value, json};

/// The package name, and its key in the lock file.
const NAME: &str = "schwitzer";
const INDEX_TS: &str = include_str!("../omp-plugin/index.ts");
const CONFIG_FILE: &str = "schwitzer.json";
const LOCK_FILE: &str = "omp-plugins.lock.json";

/// What the plugin reads from `schwitzer.json`. It holds no token: the plugin reads
/// `<data_dir>/mcp-token` on every call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginConfig {
    pub data_dir: PathBuf,
    pub mcp_url: String,
    /// The rooms (id in hex) the repo is shared with.
    pub rooms: Vec<String>,
}

fn plugin_dir(repo: &Path) -> PathBuf {
    repo.join(".omp/plugins/node_modules").join(NAME)
}

fn lock_path(repo: &Path) -> PathBuf {
    repo.join(".omp/plugins").join(LOCK_FILE)
}

/// The rooms (keys of `repos`, in map order) that share `repo`.
pub fn rooms_of(repos: &BTreeMap<String, Vec<PathBuf>>, repo: &Path) -> Vec<String> {
    repos
        .iter()
        .filter(|(_, paths)| paths.iter().any(|path| path == repo))
        .map(|(room, _)| room.clone())
        .collect()
}

/// Whether the app's plugin is in `repo`.
pub fn installed(repo: &Path) -> bool {
    plugin_dir(repo).join(CONFIG_FILE).is_file()
}

/// Writes the plugin into `repo` and enables it in the lock file. A lock file that is not an
/// omp lock file is an error, and then nothing is written.
pub fn install(repo: &Path, config: &PluginConfig) -> io::Result<()> {
    let lock_path = lock_path(repo);
    let mut lock = read_lock(&lock_path)?.unwrap_or_default();
    // `read_lock` accepts only an object as `plugins`.
    if let Some(plugins) = lock
        .entry("plugins")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    {
        let old = plugins.get(NAME);
        let entry = json!({
            "version": env!("CARGO_PKG_VERSION"),
            "enabledFeatures": old.and_then(|o| o.get("enabledFeatures")).cloned().unwrap_or(Value::Null),
            "enabled": old.and_then(|o| o.get("enabled")).and_then(Value::as_bool).unwrap_or(true),
        });
        plugins.insert(NAME.to_owned(), entry);
    }
    lock.entry("settings")
        .or_insert_with(|| Value::Object(Map::new()));

    let dir = plugin_dir(repo);
    std::fs::create_dir_all(&dir)?;
    write_if_changed(&dir.join("index.ts"), INDEX_TS.as_bytes())?;
    let package = json!({
        "name": NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "private": true,
        "description": "Plan Schwitzer tasks from omp. Written by the Schwitzer app; edits are overwritten.",
        "omp": { "extensions": ["./index.ts"] },
    });
    write_if_changed(&dir.join("package.json"), pretty(&package)?.as_bytes())?;
    write_if_changed(&dir.join(CONFIG_FILE), pretty(config)?.as_bytes())?;
    write_lock(&lock_path, lock)
}

/// Removes the plugin from `repo` and its entries from the lock file. A lock file that is not
/// an omp lock file is left alone: omp skips an entry whose directory is gone.
pub fn uninstall(repo: &Path) -> io::Result<()> {
    let dir = plugin_dir(repo);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let lock_path = lock_path(repo);
    let Ok(Some(mut lock)) = read_lock(&lock_path) else {
        return Ok(());
    };
    for section in ["plugins", "settings"] {
        if let Some(Value::Object(entries)) = lock.get_mut(section) {
            entries.remove(NAME);
        }
    }
    write_lock(&lock_path, lock)
}

/// The lock file as an object whose `plugins` is missing or an object; `None` when it does
/// not exist.
fn read_lock(path: &Path) -> io::Result<Option<Map<String, Value>>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    match serde_json::from_str(&text) {
        Ok(Value::Object(lock)) if lock.get("plugins").is_none_or(Value::is_object) => {
            Ok(Some(lock))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not an omp plugin lock file", path.display()),
        )),
    }
}

/// Writes the lock file through a temporary file and a rename.
fn write_lock(path: &Path, lock: Map<String, Value>) -> io::Result<()> {
    let tmp = path.with_file_name(format!("{LOCK_FILE}.tmp"));
    std::fs::write(&tmp, pretty(&Value::Object(lock))?)?;
    std::fs::rename(&tmp, path)
}

fn pretty(value: &impl Serialize) -> io::Result<String> {
    serde_json::to_string_pretty(value).map_err(io::Error::other)
}

/// Writes `bytes` unless the file already holds them, so an unchanged plugin keeps its mtime.
fn write_if_changed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PluginConfig {
        PluginConfig {
            data_dir: PathBuf::from("/data"),
            mcp_url: "http://127.0.0.1:13401/mcp".to_owned(),
            rooms: vec!["ab".repeat(32)],
        }
    }

    fn lock(repo: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(lock_path(repo)).unwrap()).unwrap()
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn install_into_bare_repo() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config()).unwrap();
        let dir = plugin_dir(repo.path());
        assert_eq!(
            std::fs::read_to_string(dir.join("index.ts")).unwrap(),
            INDEX_TS
        );
        let package: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).unwrap())
                .unwrap();
        assert_eq!(package["omp"]["extensions"], json!(["./index.ts"]));
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(CONFIG_FILE)).unwrap()).unwrap();
        assert_eq!(written, serde_json::to_value(config()).unwrap());
        assert_eq!(
            lock(repo.path()),
            json!({
                "plugins": { "schwitzer": {
                    "enabled": true,
                    "enabledFeatures": null,
                    "version": env!("CARGO_PKG_VERSION"),
                } },
                "settings": {},
            })
        );
        assert!(installed(repo.path()));
    }

    #[test]
    fn install_keeps_other_plugins_and_disabled_state() {
        let repo = tempfile::tempdir().unwrap();
        let other = json!({ "version": "1.0.0", "enabledFeatures": ["x"], "enabled": true });
        write(
            &lock_path(repo.path()),
            &json!({
                "plugins": {
                    "other": other,
                    "schwitzer": { "version": "0.0.1", "enabledFeatures": null, "enabled": false },
                },
                "settings": { "other": { "k": 1 } },
            })
            .to_string(),
        );
        install(repo.path(), &config()).unwrap();
        let lock = lock(repo.path());
        assert_eq!(lock["plugins"]["other"], other);
        assert_eq!(lock["settings"], json!({ "other": { "k": 1 } }));
        assert_eq!(lock["plugins"]["schwitzer"]["enabled"], json!(false));
        assert_eq!(
            lock["plugins"]["schwitzer"]["version"],
            json!(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn install_refuses_malformed_lock() {
        let repo = tempfile::tempdir().unwrap();
        write(&lock_path(repo.path()), "not json");
        let e = install(repo.path(), &config()).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            std::fs::read_to_string(lock_path(repo.path())).unwrap(),
            "not json"
        );
        assert!(!plugin_dir(repo.path()).exists());
    }

    #[test]
    fn uninstall_keeps_other_plugins() {
        let repo = tempfile::tempdir().unwrap();
        let other = json!({ "version": "1.0.0", "enabledFeatures": null, "enabled": true });
        write(
            &lock_path(repo.path()),
            &json!({
                "plugins": { "other": other },
                "settings": { "schwitzer": { "k": 1 }, "other": {} },
            })
            .to_string(),
        );
        install(repo.path(), &config()).unwrap();
        uninstall(repo.path()).unwrap();
        assert!(!plugin_dir(repo.path()).exists());
        assert!(!installed(repo.path()));
        assert_eq!(
            lock(repo.path()),
            json!({ "plugins": { "other": other }, "settings": { "other": {} } })
        );
    }

    #[test]
    fn rooms_of_matches_whole_paths() {
        let repos = BTreeMap::from([
            ("a".to_owned(), vec![PathBuf::from("/code/app")]),
            ("b".to_owned(), vec![PathBuf::from("/code/app-old")]),
            (
                "c".to_owned(),
                vec![PathBuf::from("/code/x"), PathBuf::from("/code/app")],
            ),
        ]);
        assert_eq!(rooms_of(&repos, Path::new("/code/app")), ["a", "c"]);
        assert!(rooms_of(&repos, Path::new("/code/other")).is_empty());
    }
}
