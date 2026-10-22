//! Persistent settings and filesystem locations.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use schwitzer_proto::{Project, RoomId};
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
    /// Whether an out-of-date omp plugin in a shared repo is rewritten as soon as the app sees it.
    pub auto_update_plugins: bool,
    /// Whether a new release is downloaded and installed as soon as the update check finds it.
    pub auto_install_updates: bool,
    /// Whether the app restarts by itself once an update is installed.
    pub auto_restart_updates: bool,
    /// Whether a system notification tells this member that one of their agents handed in a
    /// plan or finished a build.
    pub notify_hand_ins: bool,
    /// Per project (key `<room id hex>/<project id>`), the local repositories linked to it;
    /// omp sessions in them are shared with the project's room.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub repos: BTreeMap<String, Vec<PathBuf>>,
    /// Per room, the project last picked in it; the app shows it again after a restart.
    /// Keys are room ids in hex; a key that is no room id is dropped on load.
    #[serde(with = "room_map", skip_serializing_if = "BTreeMap::is_empty")]
    pub chosen_projects: BTreeMap<RoomId, u64>,
    /// The folders whose subfolders are matched against project names when a repo is linked.
    /// `None` until the app first starts with this setting; it then enters the default folders
    /// that exist (see [`default_projects_dirs`]). `Some(vec![])` means the member removed them all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projects_dirs: Option<Vec<PathBuf>>,
    /// The WSL distros whose default projects folders (see [`default_projects_dirs`]) were
    /// entered into `projects_dirs`. Each distro is entered once, so a folder the member
    /// removes stays removed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wsl_distros: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: default_name(),
            mcp_port: DEFAULT_MCP_PORT,
            omp_sessions_dir: default_sessions_dir(),
            last_server: None,
            rooms: Vec::new(),
            auto_update_plugins: true,
            auto_install_updates: true,
            auto_restart_updates: true,
            notify_hand_ins: false,
            repos: BTreeMap::new(),
            chosen_projects: BTreeMap::new(),
            projects_dirs: None,
            wsl_distros: Vec::new(),
        }
    }
}

/// `BTreeMap<RoomId, u64>` as a TOML table keyed by room id hex.
mod room_map {
    use std::collections::BTreeMap;

    use schwitzer_proto::RoomId;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(map: &BTreeMap<RoomId, u64>, s: S) -> Result<S::Ok, S::Error> {
        s.collect_map(
            map.iter()
                .map(|(room, project)| (room.to_string(), project)),
        )
    }

    /// A key that is no room id is dropped: the entry only remembers a choice.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<RoomId, u64>, D::Error> {
        let raw = BTreeMap::<String, u64>::deserialize(d)?;
        Ok(raw
            .into_iter()
            .filter_map(|(key, project)| Some((key.parse().ok()?, project)))
            .collect())
    }
}

/// The folders `code`, `projects` and `work` under `home` that exist, in that order.
pub fn default_projects_dirs(home: &Path) -> Vec<PathBuf> {
    ["code", "projects", "work"]
        .into_iter()
        .map(|name| home.join(name))
        .filter(|dir| dir.is_dir())
        .collect()
}

/// The visible subfolders of `dir`, sorted. Errors give none: a missing folder only means no
/// suggestions.
pub fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

impl Settings {
    /// Reads `settings.toml` from `dir`. A missing file gives the defaults; a file that cannot
    /// be read or parsed gives the defaults and the reason. The file is left alone: only
    /// [`Settings::save`] writes it.
    pub fn load(dir: &Path) -> (Self, Option<String>) {
        let path = dir.join(FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Settings>(&text) {
                Ok(mut settings) => {
                    settings.migrate_room_keys();
                    (settings, None)
                }
                Err(e) => (Self::default(), Some(format!("{}: {e}", path.display()))),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => (Self::default(), None),
            Err(e) => (Self::default(), Some(format!("{}: {e}", path.display()))),
        }
    }

    /// Before projects, a `repos` key was the room alone; the server's schema migration gave
    /// every room of that time project 1, so such keys move there.
    fn migrate_room_keys(&mut self) {
        let old: Vec<String> = self
            .repos
            .keys()
            .filter(|key| !key.contains('/'))
            .cloned()
            .collect();
        for key in old {
            let Some(paths) = self.repos.remove(&key) else {
                continue;
            };
            let linked = self.repos.entry(format!("{key}/1")).or_default();
            for path in paths {
                if !linked.contains(&path) {
                    linked.push(path);
                }
            }
        }
    }

    /// Enters `dirs`, the default projects folders of the WSL distro `distro`, into the
    /// projects folders, unless that distro was entered before (names compared ASCII
    /// case-insensitively). A folder already listed is not listed twice. `true` when the
    /// settings changed.
    pub fn enter_wsl_distro(&mut self, distro: &str, dirs: &[PathBuf]) -> bool {
        if self
            .wsl_distros
            .iter()
            .any(|name| name.eq_ignore_ascii_case(distro))
        {
            return false;
        }
        self.wsl_distros.push(distro.to_owned());
        let list = self.projects_dirs.get_or_insert_with(Vec::new);
        for dir in dirs {
            if !list.contains(dir) {
                list.push(dir.clone());
            }
        }
        true
    }

    /// The repos linked to `project` of `room`.
    pub fn project_repos(&self, room: &RoomId, project: u64) -> &[PathBuf] {
        self.repos
            .get(&repo_key(room, project))
            .map_or(&[], Vec::as_slice)
    }

    /// The projects of `room` with linked repos, and those repos, ordered by project id.
    pub fn room_project_repos(&self, room: &RoomId) -> Vec<(u64, &[PathBuf])> {
        let hex = room.to_hex();
        let mut linked: Vec<(u64, &[PathBuf])> = self
            .repos
            .iter()
            .filter_map(|(key, paths)| match parse_repo_key(key) {
                Some((r, project)) if r == hex => Some((project, paths.as_slice())),
                _ => None,
            })
            .collect();
        linked.sort_by_key(|(project, _)| *project);
        linked
    }

    /// Forgets the repos linked to, and the choice of, projects of `room` that are not in
    /// `projects` (the room's current list) any more; `true` when anything changed.
    pub fn forget_deleted_projects(&mut self, room: &RoomId, projects: &[Project]) -> bool {
        let hex = room.to_hex();
        let exists = |id: u64| projects.iter().any(|p| p.id == id);
        let before = (self.repos.len(), self.chosen_projects.len());
        self.repos.retain(|key, _| match parse_repo_key(key) {
            Some((r, id)) => r != hex || exists(id),
            None => true,
        });
        if self
            .chosen_projects
            .get(room)
            .is_some_and(|&id| !exists(id))
        {
            self.chosen_projects.remove(room);
        }
        before != (self.repos.len(), self.chosen_projects.len())
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

/// The `repos` key of `project` in `room`: `<room id hex>/<project id>`.
pub fn repo_key(room: &RoomId, project: u64) -> String {
    format!("{}/{project}", room.to_hex())
}

/// The room id in hex and the project id of a `repos` key; `None` if it is malformed.
pub fn parse_repo_key(key: &str) -> Option<(&str, u64)> {
    let (room, project) = key.rsplit_once('/')?;
    let hex = room.len() == 64 && room.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex {
        return None;
    }
    Some((room, project.parse().ok()?))
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
        assert!(settings.auto_update_plugins);
        assert!(settings.auto_install_updates);
        assert!(settings.auto_restart_updates);
        assert!(!settings.notify_hand_ins);
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
            auto_update_plugins: false,
            auto_install_updates: false,
            auto_restart_updates: false,
            notify_hand_ins: true,
            repos: BTreeMap::from([(
                format!("{}/1", "ab".repeat(32)),
                vec![PathBuf::from("/code/x")],
            )]),
            chosen_projects: BTreeMap::from([(RoomId([0xab; 32]), 2)]),
            projects_dirs: Some(vec![PathBuf::from("/code"), PathBuf::from("/work")]),
            wsl_distros: vec!["Ubuntu".to_owned()],
        };
        settings.save(dir.path()).unwrap();
        assert!(!dir.path().join("settings.toml.tmp").exists());
        assert_eq!(Settings::load(dir.path()), (settings, None));
    }

    #[test]
    fn chosen_projects_keep_room_ids_and_drop_bad_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(FILE),
            format!(
                "[chosen_projects]\n\"{}\" = 3\n\"nope\" = 1\n",
                "ab".repeat(32)
            ),
        )
        .unwrap();
        let (settings, problem) = Settings::load(dir.path());
        assert_eq!(problem, None);
        assert_eq!(
            settings.chosen_projects,
            BTreeMap::from([(RoomId([0xab; 32]), 3)])
        );
    }

    #[test]
    fn default_projects_dirs_are_the_existing_ones() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("code")).unwrap();
        std::fs::create_dir(dir.path().join("work")).unwrap();
        std::fs::write(dir.path().join("projects"), "").unwrap();
        assert_eq!(
            default_projects_dirs(dir.path()),
            [dir.path().join("code"), dir.path().join("work")]
        );
    }

    #[test]
    fn wsl_distro_is_entered_once() {
        let code = PathBuf::from("/code");
        let ubuntu_code = PathBuf::from(r"\\wsl.localhost\Ubuntu\home\felix\code");
        let ubuntu_work = PathBuf::from(r"\\wsl.localhost\Ubuntu\home\felix\work");
        let mut settings = Settings {
            projects_dirs: Some(vec![code.clone()]),
            ..Settings::default()
        };
        let both = [ubuntu_code.clone(), ubuntu_work.clone()];
        assert!(settings.enter_wsl_distro("Ubuntu", &both));
        assert_eq!(
            settings.projects_dirs.as_deref(),
            Some(&[code.clone(), ubuntu_code.clone(), ubuntu_work.clone()][..])
        );
        if let Some(list) = settings.projects_dirs.as_mut() {
            list.retain(|dir| *dir != ubuntu_code);
        }
        assert!(!settings.enter_wsl_distro("ubuntu", &both));
        assert_eq!(
            settings.projects_dirs.as_deref(),
            Some(&[code.clone(), ubuntu_work.clone()][..])
        );
        assert!(settings.enter_wsl_distro("Debian", std::slice::from_ref(&ubuntu_work)));
        assert_eq!(
            settings.projects_dirs.as_deref(),
            Some(&[code, ubuntu_work][..])
        );
        assert_eq!(settings.wsl_distros, ["Ubuntu", "Debian"]);
    }

    #[test]
    fn room_keys_from_before_projects_move_to_project_one() {
        let dir = tempfile::tempdir().unwrap();
        let hex = "ab".repeat(32);
        std::fs::write(
            dir.path().join(FILE),
            format!("[repos]\n\"{hex}\" = [\"/a\"]\n\"{hex}/1\" = [\"/a\", \"/b\"]\n"),
        )
        .unwrap();
        let (settings, problem) = Settings::load(dir.path());
        assert_eq!(problem, None);
        assert_eq!(
            settings.repos,
            BTreeMap::from([(
                format!("{hex}/1"),
                vec![PathBuf::from("/a"), PathBuf::from("/b")]
            )])
        );
    }

    #[test]
    fn repo_keys_need_a_room_id_and_a_project_number() {
        let hex = "ab".repeat(32);
        assert_eq!(parse_repo_key(&format!("{hex}/3")), Some((hex.as_str(), 3)));
        for bad in [
            hex.clone(),
            format!("{hex}/x"),
            "ab/1".to_owned(),
            format!("{}/1", "zz".repeat(32)),
        ] {
            assert_eq!(parse_repo_key(&bad), None, "{bad}");
        }
    }

    #[test]
    fn forgetting_deleted_projects_keeps_other_rooms() {
        let (a, b) = (RoomId([1; 32]), RoomId([2; 32]));
        let mut settings = Settings {
            repos: BTreeMap::from([
                (repo_key(&a, 1), vec![PathBuf::from("/app")]),
                (repo_key(&a, 2), vec![PathBuf::from("/old")]),
                (repo_key(&b, 2), vec![PathBuf::from("/other")]),
            ]),
            chosen_projects: BTreeMap::from([(a, 2)]),
            ..Settings::default()
        };
        let projects = [Project {
            id: 1,
            name: "app".into(),
        }];
        assert!(settings.forget_deleted_projects(&a, &projects));
        assert_eq!(
            settings.repos.keys().cloned().collect::<Vec<_>>(),
            [repo_key(&a, 1), repo_key(&b, 2)]
        );
        assert!(settings.chosen_projects.is_empty());
        assert!(!settings.forget_deleted_projects(&a, &projects));
    }
}
