//! The omp plugin in shared repos.
//!
//! The app writes the plugin (`omp-plugin/index.ts`, a `package.json` and `schwitzer.json`)
//! into `<repo>/.omp/plugins/node_modules/schwitzer/` and enters it in
//! `<repo>/.omp/plugins/omp-plugins.lock.json`, so omp started in the repo loads it as a
//! project-scope plugin. It gives omp `/schwitzer plan`, `build`, `propose` and `idea`, which
//! plan, build and propose tasks of the projects the repo is linked to and note ideas for them
//! through the app's MCP server. `omp-plugin/README.md` describes the plugin.
//!
//! The plugin gives omp's agent the app's tools itself and reaches the app's MCP server with
//! the URL in `schwitzer.json` and the token in `<data_dir>/mcp-token`.
//!
//! The app compares a repo's plugin with what it would write ([`state`]) and rewrites it on
//! request or automatically.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::{settings, wsl};

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
    /// The projects the repo is linked to.
    pub projects: Vec<PluginProject>,
    /// The repos of the projects this repo is not linked to; the plugin asks before the agent
    /// edits a file in one of them.
    pub other_repos: Vec<OtherRepo>,
}

/// One project a repo is linked to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginProject {
    /// The room id in hex.
    pub room: String,
    pub project: u64,
    /// Whether builds in this project need `Schwitzer-Task: #<n>` in every commit; see
    /// [`Settings::commit_trailers`](crate::settings::Settings::commit_trailers).
    pub commit_trailer: bool,
}

/// A repo linked to a project the plugin's repo is not linked to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OtherRepo {
    /// The room id in hex.
    pub room: String,
    pub project: u64,
    /// The repo's path as the plugin sees it ([`wsl::path_for`]).
    pub path: PathBuf,
}

fn plugin_dir(repo: &Path) -> PathBuf {
    repo.join(".omp/plugins/node_modules").join(NAME)
}

fn lock_path(repo: &Path) -> PathBuf {
    repo.join(".omp/plugins").join(LOCK_FILE)
}

/// The projects (keys of `repos`, in map order) that `repo` is linked to; malformed keys are
/// skipped. `trailers` holds the keys whose commit trailer switch is on.
pub fn projects_of(
    repos: &BTreeMap<String, Vec<PathBuf>>,
    trailers: &BTreeSet<String>,
    repo: &Path,
) -> Vec<PluginProject> {
    repos
        .iter()
        .filter(|(_, paths)| paths.iter().any(|path| path == repo))
        .filter_map(|(key, _)| {
            let (room, project) = settings::parse_repo_key(key)?;
            Some(PluginProject {
                room: room.to_owned(),
                project,
                commit_trailer: trailers.contains(key),
            })
        })
        .collect()
}

/// The repos of the projects `repo` is not linked to, in map order, as the plugin in `repo`
/// sees them ([`wsl::path_for`]). Malformed keys are skipped, and so are a repo of one of
/// `repo`'s own projects (also `repo` itself) and a repo in another WSL distro than `repo`,
/// which the plugin cannot reach.
pub fn other_repos(repos: &BTreeMap<String, Vec<PathBuf>>, repo: &Path) -> Vec<OtherRepo> {
    // Only room and project are compared here, so the trailer switches do not matter.
    let own = projects_of(repos, &BTreeSet::new(), repo);
    let is_own = |room: &str, project| own.iter().any(|p| p.room == room && p.project == project);
    let own_paths: Vec<&PathBuf> = repos
        .iter()
        .filter(|(key, _)| {
            settings::parse_repo_key(key).is_some_and(|(room, project)| is_own(room, project))
        })
        .flat_map(|(_, paths)| paths)
        .collect();
    let repo_distro = wsl::distro_of(repo);
    let mut others = Vec::new();
    for (key, paths) in repos {
        let Some((room, project)) = settings::parse_repo_key(key) else {
            continue;
        };
        if is_own(room, project) {
            continue;
        }
        for path in paths {
            if own_paths.contains(&path) {
                continue;
            }
            if let (Some(theirs), Some(ours)) = (wsl::distro_of(path), repo_distro)
                && !theirs.eq_ignore_ascii_case(ours)
            {
                continue;
            }
            others.push(OtherRepo {
                room: room.to_owned(),
                project,
                path: wsl::path_for(repo, path),
            });
        }
    }
    others
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

/// The plugin in `repo` compared with what [`install`] would write for `config`; `None` when
/// it is not installed.
pub fn state(repo: &Path, config: &PluginConfig) -> Option<PluginState> {
    if !installed(repo) {
        return None;
    }
    let current = if matches(repo, config) {
        PluginState::Current
    } else {
        PluginState::Outdated
    };
    Some(current)
}

/// How many of `states` are out of date.
pub fn count_outdated<'a>(states: impl IntoIterator<Item = &'a PluginState>) -> usize {
    states
        .into_iter()
        .filter(|state| **state == PluginState::Outdated)
        .count()
}

/// Whether everything [`install`] would write into `repo` is already there.
fn matches(repo: &Path, config: &PluginConfig) -> bool {
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
    match read_json(&lock_path(repo), "plugins", "an omp plugin lock file") {
        Ok(Some(lock)) => {
            lock.get("plugins")
                .and_then(|plugins| plugins.get(NAME))
                .and_then(|entry| entry.get("version"))
                == Some(&json!(env!("CARGO_PKG_VERSION")))
        }
        _ => false,
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

/// Writes the plugin into `repo` and enables it in the lock file. A lock file that is not an
/// omp lock file is an error, and then nothing is written.
pub fn install(repo: &Path, config: &PluginConfig) -> io::Result<()> {
    let lock_path = lock_path(repo);
    let mut lock = read_json(&lock_path, "plugins", "an omp plugin lock file")?.unwrap_or_default();
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

    let dir = plugin_dir(repo);
    std::fs::create_dir_all(&dir)?;
    write_if_changed(&dir.join("index.ts"), INDEX_TS.as_bytes())?;
    write_if_changed(&dir.join("package.json"), pretty(&package())?.as_bytes())?;
    write_if_changed(&dir.join(CONFIG_FILE), pretty(config)?.as_bytes())?;
    write_json(&lock_path, lock)?;
    Ok(())
}

/// Rewrites only `schwitzer.json` in `repo`'s plugin, so a changed project switch reaches
/// a running omp at once; `index.ts` and the lock file stay as they are.
pub fn write_config(repo: &Path, config: &PluginConfig) -> io::Result<()> {
    write_if_changed(
        &plugin_dir(repo).join(CONFIG_FILE),
        pretty(config)?.as_bytes(),
    )
}

/// Removes the plugin from `repo` and its entries from the lock file. A file that omp would
/// not read is left alone: omp skips a lock entry whose directory is gone.
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
        write_json(&lock_path, lock)?;
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

/// Writes the JSON file at `path` through a temporary file and a rename.
fn write_json(path: &Path, value: Map<String, Value>) -> io::Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!("{name}.tmp"));
    std::fs::write(&tmp, pretty(&Value::Object(value))?.as_bytes())?;
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
            projects: vec![PluginProject {
                room: "ab".repeat(32),
                project: 1,
                commit_trailer: false,
            }],
            other_repos: vec![],
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
    fn projects_of_matches_whole_paths() {
        let (a, c) = ("aa".repeat(32), "cc".repeat(32));
        let repos = BTreeMap::from([
            (format!("{a}/1"), vec![PathBuf::from("/code/app")]),
            (format!("{a}/2"), vec![PathBuf::from("/code/app-old")]),
            (
                format!("{c}/3"),
                vec![PathBuf::from("/code/x"), PathBuf::from("/code/app")],
            ),
            ("malformed".to_owned(), vec![PathBuf::from("/code/app")]),
        ]);
        let linked = |room: &str, project, commit_trailer| PluginProject {
            room: room.to_owned(),
            project,
            commit_trailer,
        };
        assert_eq!(
            projects_of(&repos, &BTreeSet::new(), Path::new("/code/app")),
            [linked(&a, 1, false), linked(&c, 3, false)]
        );
        assert_eq!(
            projects_of(
                &repos,
                &BTreeSet::from([format!("{c}/3")]),
                Path::new("/code/app")
            ),
            [linked(&a, 1, false), linked(&c, 3, true)]
        );
        assert!(projects_of(&repos, &BTreeSet::new(), Path::new("/code/other")).is_empty());
    }

    #[test]
    fn other_repos_skip_own_projects() {
        let (a, c) = ("aa".repeat(32), "cc".repeat(32));
        let repos = BTreeMap::from([
            (format!("{a}/1"), vec![PathBuf::from("/code/app")]),
            (format!("{a}/2"), vec![PathBuf::from("/code/lib")]),
            (
                format!("{c}/3"),
                vec![PathBuf::from("/code/shared"), PathBuf::from("/code/app")],
            ),
            (
                format!("{c}/4"),
                vec![PathBuf::from("/code/shared"), PathBuf::from("/code/tool")],
            ),
            ("malformed".to_owned(), vec![PathBuf::from("/code/x")]),
        ]);
        let other = |room: &str, project, path: &str| OtherRepo {
            room: room.to_owned(),
            project,
            path: PathBuf::from(path),
        };
        assert_eq!(
            other_repos(&repos, Path::new("/code/app")),
            [other(&a, 2, "/code/lib"), other(&c, 4, "/code/tool")]
        );
        assert_eq!(
            other_repos(&repos, Path::new("/code/none")),
            [
                other(&a, 1, "/code/app"),
                other(&a, 2, "/code/lib"),
                other(&c, 3, "/code/shared"),
                other(&c, 3, "/code/app"),
                other(&c, 4, "/code/shared"),
                other(&c, 4, "/code/tool"),
            ]
        );
    }

    #[test]
    fn other_repos_as_seen_from_a_wsl_repo() {
        let a = "aa".repeat(32);
        let repos = BTreeMap::from([
            (
                format!("{a}/1"),
                vec![PathBuf::from(r"\\wsl.localhost\Ubuntu\code\app")],
            ),
            (
                format!("{a}/2"),
                vec![
                    PathBuf::from(r"\\wsl.localhost\Ubuntu\code\lib"),
                    PathBuf::from(r"\\wsl.localhost\Debian\code\x"),
                    PathBuf::from(r"C:\code\win"),
                ],
            ),
        ]);
        let paths: Vec<PathBuf> =
            other_repos(&repos, Path::new(r"\\wsl.localhost\Ubuntu\code\app"))
                .into_iter()
                .map(|other| other.path)
                .collect();
        assert_eq!(
            paths,
            [Path::new("/code/lib"), Path::new("/mnt/c/code/win")]
        );
    }

    #[test]
    fn state_follows_install() {
        let repo = tempfile::tempdir().unwrap();
        assert_eq!(state(repo.path(), &config()), None);
        install(repo.path(), &config()).unwrap();
        assert_eq!(state(repo.path(), &config()), Some(PluginState::Current));
    }

    #[test]
    fn edited_plugin_is_outdated_until_reinstalled() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config()).unwrap();
        let index = plugin_dir(repo.path()).join("index.ts");
        std::fs::write(&index, format!("{INDEX_TS}// edited\n")).unwrap();
        assert_eq!(state(repo.path(), &config()), Some(PluginState::Outdated));
        install(repo.path(), &config()).unwrap();
        assert_eq!(state(repo.path(), &config()), Some(PluginState::Current));
    }

    #[test]
    fn other_mcp_port_is_outdated() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config()).unwrap();
        let moved = PluginConfig {
            mcp_url: "http://127.0.0.1:13402/mcp".to_owned(),
            ..config()
        };
        assert_eq!(state(repo.path(), &moved), Some(PluginState::Outdated));
    }

    #[test]
    fn older_lock_version_is_outdated() {
        let repo = tempfile::tempdir().unwrap();
        install(repo.path(), &config()).unwrap();
        let mut lock = lock(repo.path());
        lock["plugins"]["schwitzer"]["version"] = json!("0.0.1");
        write(&lock_path(repo.path()), &lock.to_string());
        assert_eq!(state(repo.path(), &config()), Some(PluginState::Outdated));
    }
}
