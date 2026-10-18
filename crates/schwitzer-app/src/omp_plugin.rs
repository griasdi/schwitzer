//! The omp plugin in shared repos.
//!
//! The app writes the plugin (`omp-plugin/index.ts`, a `package.json` and `schwitzer.json`)
//! into `<repo>/.omp/plugins/node_modules/schwitzer/` and enters it in
//! `<repo>/.omp/plugins/omp-plugins.lock.json`, so omp started in the repo loads it as a
//! project-scope plugin. It gives omp `/schwitzer plan`, `build` and `propose`, which plan,
//! build and propose tasks of the rooms the repo is shared with through the app's MCP server.
//!
//! Install also registers the app's MCP server as `schwitzer` in `<repo>/.omp/mcp.json`, with
//! the access token in its `Authorization` header. That file is then readable by the user only
//! on unix and must stay out of version control.
//!
//! The app compares a repo's plugin with what it would write ([`state`]) and rewrites it on
//! request or automatically.

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
const MCP_FILE: &str = "mcp.json";
const MCP_SCHEMA: &str = "https://raw.githubusercontent.com/can1357/oh-my-pi/main/packages/coding-agent/src/config/mcp-schema.json";

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

fn mcp_path(repo: &Path) -> PathBuf {
    repo.join(".omp").join(MCP_FILE)
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

/// How the plugin in a repo compares with the one this app writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    Current,
    Outdated,
}

/// The plugin in `repo` compared with what [`install`] would write for `config` and
/// `mcp_token`; `None` when it is not installed.
pub fn state(repo: &Path, config: &PluginConfig, mcp_token: Option<&str>) -> Option<PluginState> {
    if !installed(repo) {
        return None;
    }
    let current = if matches(repo, config, mcp_token) {
        PluginState::Current
    } else {
        PluginState::Outdated
    };
    Some(current)
}

/// Whether everything [`install`] would write into `repo` is already there.
fn matches(repo: &Path, config: &PluginConfig, mcp_token: Option<&str>) -> bool {
    let dir = plugin_dir(repo);
    let holds =
        |name: &str, bytes: &[u8]| std::fs::read(dir.join(name)).is_ok_and(|old| old == bytes);
    let (Ok(package), Ok(config_json)) = (pretty(&package()), pretty(config)) else {
        return false;
    };
    if !holds("index.ts", INDEX_TS.as_bytes())
        || !holds("package.json", package.as_bytes())
        || !holds(CONFIG_FILE, config_json.as_bytes())
    {
        return false;
    }
    let lock_current = match read_json(&lock_path(repo), "plugins", "an omp plugin lock file") {
        Ok(Some(lock)) => {
            lock.get("plugins")
                .and_then(|plugins| plugins.get(NAME))
                .and_then(|entry| entry.get("version"))
                == Some(&json!(env!("CARGO_PKG_VERSION")))
        }
        _ => false,
    };
    if !lock_current {
        return false;
    }
    let entry = match read_json(&mcp_path(repo), "mcpServers", "an omp MCP config") {
        Ok(mcp) => mcp.and_then(|mcp| mcp.get("mcpServers")?.get(NAME).cloned()),
        Err(_) => return false,
    };
    match mcp_token {
        Some(token) => entry == Some(crate::mcp::server_entry(&config.mcp_url, token)),
        None => entry.is_none(),
    }
}

/// The plugin's `package.json`.
fn package() -> Value {
    json!({
        "name": NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "private": true,
        "description": "Plan Schwitzer tasks from omp. Written by the Schwitzer app; edits are overwritten.",
        "omp": { "extensions": ["./index.ts"] },
    })
}

/// Writes the plugin into `repo`, enables it in the lock file and registers the app's MCP
/// server with `mcp_token` as `schwitzer` in `.omp/mcp.json`; without a token (the server
/// cannot run) the entry is removed. A lock file that is not an omp lock file or an
/// `mcp.json` that is not an omp MCP config is an error, and then nothing is written.
pub fn install(repo: &Path, config: &PluginConfig, mcp_token: Option<&str>) -> io::Result<()> {
    let lock_path = lock_path(repo);
    let mut lock = read_json(&lock_path, "plugins", "an omp plugin lock file")?.unwrap_or_default();
    let mcp_path = mcp_path(repo);
    let mut mcp = read_json(&mcp_path, "mcpServers", "an omp MCP config")?;
    // `read_json` accepts only an object as `plugins`.
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
    // `read_json` accepts only an object as `mcpServers`.
    match (mcp_token, &mut mcp) {
        (Some(token), mcp) => {
            let mcp = mcp
                .get_or_insert_with(|| Map::from_iter([("$schema".to_owned(), json!(MCP_SCHEMA))]));
            if let Some(servers) = mcp
                .entry("mcpServers")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
            {
                servers.insert(
                    NAME.to_owned(),
                    crate::mcp::server_entry(&config.mcp_url, token),
                );
            }
        }
        (None, Some(mcp)) => {
            if let Some(Value::Object(servers)) = mcp.get_mut("mcpServers") {
                servers.remove(NAME);
            }
        }
        (None, None) => {}
    }

    let dir = plugin_dir(repo);
    std::fs::create_dir_all(&dir)?;
    write_if_changed(&dir.join("index.ts"), INDEX_TS.as_bytes())?;
    write_if_changed(&dir.join("package.json"), pretty(&package())?.as_bytes())?;
    write_if_changed(&dir.join(CONFIG_FILE), pretty(config)?.as_bytes())?;
    write_json(&lock_path, lock, false)?;
    if let Some(mcp) = mcp {
        write_json(&mcp_path, mcp, true)?;
    }
    Ok(())
}

/// Removes the plugin from `repo`, its entries from the lock file and the `schwitzer` server
/// from `.omp/mcp.json`. A file that omp would not read is left alone: omp skips a lock entry
/// whose directory is gone.
pub fn uninstall(repo: &Path) -> io::Result<()> {
    let dir = plugin_dir(repo);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let lock_path = lock_path(repo);
    if let Ok(Some(mut lock)) = read_json(&lock_path, "plugins", "an omp plugin lock file") {
        for section in ["plugins", "settings"] {
            if let Some(Value::Object(entries)) = lock.get_mut(section) {
                entries.remove(NAME);
            }
        }
        write_json(&lock_path, lock, false)?;
    }
    let mcp_path = mcp_path(repo);
    if let Ok(Some(mut mcp)) = read_json(&mcp_path, "mcpServers", "an omp MCP config") {
        if let Some(Value::Object(servers)) = mcp.get_mut("mcpServers") {
            servers.remove(NAME);
        }
        write_json(&mcp_path, mcp, true)?;
    }
    Ok(())
}

/// The JSON file at `path` as an object whose `section` is missing or an object; `None` when
/// it does not exist. Anything else is an `InvalidData` error saying the file is not `what`.
fn read_json(path: &Path, section: &str, what: &str) -> io::Result<Option<Map<String, Value>>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    match serde_json::from_str(&text) {
        Ok(Value::Object(map)) if map.get(section).is_none_or(Value::is_object) => Ok(Some(map)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not {what}", path.display()),
        )),
    }
}

/// Writes the JSON file at `path` through a temporary file and a rename; readable by the user
/// only on unix when `private`, since `mcp.json` holds the access token.
fn write_json(path: &Path, value: Map<String, Value>, private: bool) -> io::Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!("{name}.tmp"));
    // Created anew, so a leftover temporary file cannot hand its mode to the token.
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    if private {
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    let mut file = options.open(&tmp)?;
    io::Write::write_all(&mut file, pretty(&Value::Object(value))?.as_bytes())?;
    drop(file);
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

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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

    fn mcp(repo: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(mcp_path(repo)).unwrap()).unwrap()
    }

    fn griasdi() -> Value {
        json!({ "mcpServers": { "griasdi": { "type": "http", "url": "http://127.0.0.1:13337/mcp" } } })
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn install_into_bare_repo() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
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
        assert_eq!(
            mcp(repo.path()),
            json!({
                "$schema": MCP_SCHEMA,
                "mcpServers": { "schwitzer": {
                    "type": "http",
                    "url": "http://127.0.0.1:13401/mcp",
                    "headers": { "Authorization": format!("Bearer {TOKEN}") },
                } },
            })
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(mcp_path(repo.path()))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
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
        write(&mcp_path(repo.path()), &griasdi().to_string());
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        let mcp = mcp(repo.path());
        assert_eq!(
            mcp["mcpServers"]["griasdi"],
            griasdi()["mcpServers"]["griasdi"]
        );
        assert_eq!(
            mcp["mcpServers"]["schwitzer"]["url"],
            json!("http://127.0.0.1:13401/mcp")
        );
        assert!(mcp.get("$schema").is_none());
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
        let e = install(repo.path(), &config(), Some(TOKEN)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            std::fs::read_to_string(lock_path(repo.path())).unwrap(),
            "not json"
        );
        assert!(!plugin_dir(repo.path()).exists());
    }

    #[test]
    fn install_refuses_malformed_mcp_config() {
        let repo = tempfile::tempdir().unwrap();
        write(&mcp_path(repo.path()), "not json");
        let e = install(repo.path(), &config(), Some(TOKEN)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            std::fs::read_to_string(mcp_path(repo.path())).unwrap(),
            "not json"
        );
        assert!(!plugin_dir(repo.path()).exists());
        assert!(!lock_path(repo.path()).exists());
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
        write(&mcp_path(repo.path()), &griasdi().to_string());
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        uninstall(repo.path()).unwrap();
        assert!(!plugin_dir(repo.path()).exists());
        assert!(!installed(repo.path()));
        assert_eq!(
            lock(repo.path()),
            json!({ "plugins": { "other": other }, "settings": { "other": {} } })
        );
        assert_eq!(mcp(repo.path()), griasdi());
    }

    #[test]
    fn install_without_token_drops_the_entry() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config(), None).unwrap();
        assert!(!mcp_path(repo.path()).exists());
        write(&mcp_path(repo.path()), &griasdi().to_string());
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        install(repo.path(), &config(), None).unwrap();
        assert_eq!(mcp(repo.path()), griasdi());
        assert!(installed(repo.path()));
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

    #[test]
    fn state_follows_install() {
        let repo = tempfile::tempdir().unwrap();
        assert_eq!(state(repo.path(), &config(), Some(TOKEN)), None);
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        assert_eq!(
            state(repo.path(), &config(), Some(TOKEN)),
            Some(PluginState::Current)
        );
    }

    #[test]
    fn edited_plugin_is_outdated_until_reinstalled() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        let index = plugin_dir(repo.path()).join("index.ts");
        std::fs::write(&index, format!("{INDEX_TS}// edited\n")).unwrap();
        assert_eq!(
            state(repo.path(), &config(), Some(TOKEN)),
            Some(PluginState::Outdated)
        );
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        assert_eq!(
            state(repo.path(), &config(), Some(TOKEN)),
            Some(PluginState::Current)
        );
    }

    #[test]
    fn other_mcp_port_is_outdated() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        let moved = PluginConfig {
            mcp_url: "http://127.0.0.1:13402/mcp".to_owned(),
            ..config()
        };
        assert_eq!(
            state(repo.path(), &moved, Some(TOKEN)),
            Some(PluginState::Outdated)
        );
    }

    #[test]
    fn older_lock_version_is_outdated() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        let mut lock = lock(repo.path());
        lock["plugins"]["schwitzer"]["version"] = json!("0.0.1");
        write(&lock_path(repo.path()), &lock.to_string());
        assert_eq!(
            state(repo.path(), &config(), Some(TOKEN)),
            Some(PluginState::Outdated)
        );
    }

    #[test]
    fn mcp_entry_follows_the_token() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        assert_eq!(
            state(repo.path(), &config(), None),
            Some(PluginState::Outdated)
        );
        install(repo.path(), &config(), None).unwrap();
        assert_eq!(
            state(repo.path(), &config(), None),
            Some(PluginState::Current)
        );
    }

    #[test]
    fn malformed_mcp_config_is_outdated() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config(), Some(TOKEN)).unwrap();
        write(&mcp_path(repo.path()), "not json");
        assert_eq!(
            state(repo.path(), &config(), Some(TOKEN)),
            Some(PluginState::Outdated)
        );
    }
}
