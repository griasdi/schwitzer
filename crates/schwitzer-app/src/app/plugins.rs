//! The omp plugin in the shared repos: install, update, remove, and the periodic check that
//! compares each installed plugin with what this app would write.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use schwitzer_proto::{RoomId, now_ms};

use super::App;
use crate::omp_plugin::{self, PluginState};
use crate::{mcp, wsl};

/// Whether the plugin is written into a repo for the first time or brought up to date; the
/// notice and the banner name it.
#[derive(Clone, Copy)]
pub(super) enum Write {
    Install,
    Update,
}

impl Write {
    fn done(self) -> &'static str {
        match self {
            Self::Install => "installed",
            Self::Update => "updated",
        }
    }

    fn failed(self, repo: &Path, e: &std::io::Error) -> String {
        let verb = match self {
            Self::Install => "install",
            Self::Update => "update",
        };
        format!("Cannot {verb} the omp plugin in {}: {e}", repo.display())
    }
}

impl App {
    /// Writes the plugin into the selected project's repo at `index`; the notice or the banner
    /// tells how it went.
    pub(super) fn write_plugin(&mut self, index: usize, write: Write) {
        let Some(repo) = self.project_repos().get(index).cloned() else {
            return;
        };
        match self.update_plugin(&repo) {
            Ok(()) => self.notify(&format!("omp plugin {}", write.done())),
            Err(e) => self.banner = Some(write.failed(&repo, &e)),
        }
    }

    pub(super) fn remove_plugin(&mut self, index: usize) {
        let Some(repo) = self.project_repos().get(index).cloned() else {
            return;
        };
        match omp_plugin::uninstall(&repo) {
            Ok(()) => {
                self.plugins.remove(&repo);
                self.auto_failed.remove(&repo);
                self.notify("omp plugin removed");
            }
            Err(e) => {
                self.banner = Some(format!(
                    "Cannot remove the omp plugin from {}: {e}",
                    repo.display()
                ));
            }
        }
    }

    /// What the plugin in `repo` is written with: the projects linked to the repo, this
    /// machine's data directory and agent port, and the other shared repos.
    fn plugin_config_for(&self, repo: &Path) -> omp_plugin::PluginConfig {
        omp_plugin::PluginConfig {
            data_dir: wsl::path_for(repo, &self.data_dir),
            mcp_url: mcp::url(self.settings.mcp_port),
            projects: omp_plugin::projects_of(
                &self.settings.repos,
                &self.settings.commit_trailers,
                repo,
            ),
            other_repos: omp_plugin::other_repos(&self.settings.repos, repo),
        }
    }

    /// Writes the plugin into `repo` for the projects it is linked to and marks it current.
    fn update_plugin(&mut self, repo: &Path) -> std::io::Result<()> {
        omp_plugin::install(repo, &self.plugin_config_for(repo))?;
        self.plugins.insert(repo.to_owned(), PluginState::Current);
        self.auto_failed.remove(repo);
        Ok(())
    }

    /// Rewrites `schwitzer.json` in every repo of `project` that has the plugin, so omp
    /// sees a changed project switch at its next commit; failures go to the banner.
    /// Config only: a full [`Self::update_plugin`] would also replace an out-of-date
    /// `index.ts` for a member who turned automatic updates off.
    pub(super) fn write_plugin_configs(&mut self, room: &RoomId, project: u64) {
        for repo in self.settings.project_repos(room, project).to_vec() {
            if !self.plugins.contains_key(&repo) {
                continue;
            }
            let config = self.plugin_config_for(&repo);
            match omp_plugin::write_config(&repo, &config) {
                Ok(()) => {
                    if let Some(state) = omp_plugin::state(&repo, &config) {
                        self.plugins.insert(repo, state);
                    }
                }
                Err(e) => self.banner = Some(format!("omp plugin in {}: {e}", repo.display())),
            }
        }
    }

    /// Updates every out-of-date plugin, in all rooms; the first failure goes to the banner.
    pub(super) fn update_all_plugins(&mut self) {
        let outdated: Vec<PathBuf> = self
            .plugins
            .iter()
            .filter(|(_, state)| **state == PluginState::Outdated)
            .map(|(repo, _)| repo.clone())
            .collect();
        let mut updated = 0;
        let mut failure = None;
        for repo in outdated {
            match self.update_plugin(&repo) {
                Ok(()) => updated += 1,
                Err(e) => {
                    failure.get_or_insert_with(|| Write::Update.failed(&repo, &e));
                }
            }
        }
        if let Some(failure) = failure {
            self.banner = Some(failure);
        }
        match updated {
            0 => {}
            1 => self.notify("omp plugin updated"),
            n => self.notify(&format!("omp plugin updated in {n} repos")),
        }
    }

    /// Compares the plugin in every shared repo with this app's and, with auto-update on,
    /// rewrites the out-of-date ones whose automatic update has not failed before.
    pub(super) fn check_plugins(&mut self) {
        self.plugins_checked_ms = now_ms();
        let shared: BTreeSet<PathBuf> = self.settings.repos.values().flatten().cloned().collect();
        for repo in &shared {
            // A repo in a stopped distro is not read: that would start the distro.
            if self.in_stopped_distro(repo) {
                continue;
            }
            match omp_plugin::state(repo, &self.plugin_config_for(repo)) {
                Some(state) => {
                    self.plugins.insert(repo.clone(), state);
                }
                None => {
                    self.plugins.remove(repo);
                }
            }
        }
        self.plugins.retain(|repo, _| shared.contains(repo));
        let plugins = &self.plugins;
        self.auto_failed
            .retain(|repo| plugins.get(repo) == Some(&PluginState::Outdated));
        if !self.settings.auto_update_plugins {
            return;
        }
        let outdated: Vec<PathBuf> = self
            .plugins
            .iter()
            .filter(|(repo, state)| {
                **state == PluginState::Outdated && !self.auto_failed.contains(*repo)
            })
            .map(|(repo, _)| repo.clone())
            .collect();
        let mut updated = false;
        for repo in outdated {
            match self.update_plugin(&repo) {
                Ok(()) => updated = true,
                Err(e) => {
                    self.auto_failed.insert(repo.clone());
                    self.banner = Some(Write::Update.failed(&repo, &e));
                }
            }
        }
        if updated {
            self.notify("omp plugin updated");
        }
    }

    /// Rewrites the plugin in every repo that has it, and removes it from a repo no room
    /// is linked to any more. Runs regardless of auto-update: `schwitzer.json` lists the
    /// projects.
    pub(super) fn sync_plugins(&mut self) {
        for repo in self.plugins.keys().cloned().collect::<Vec<_>>() {
            let result = if omp_plugin::projects_of(
                &self.settings.repos,
                &self.settings.commit_trailers,
                &repo,
            )
            .is_empty()
            {
                omp_plugin::uninstall(&repo).map(|()| {
                    self.plugins.remove(&repo);
                    self.auto_failed.remove(&repo);
                })
            } else {
                self.update_plugin(&repo)
            };
            if let Err(e) = result {
                self.banner = Some(format!("omp plugin in {}: {e}", repo.display()));
            }
        }
    }
}
