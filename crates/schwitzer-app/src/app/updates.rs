//! The self-update: which release to look for, the check, the install and what follows.

use iced::Task;
use schwitzer_release::Channel;

use super::logic::{self, Check};
use super::{App, Message};
use crate::update::{self, UpdateState};

impl App {
    /// The update channel: the member's choice, else the one of this build.
    pub(crate) fn channel(&self) -> Channel {
        self.settings
            .update_channel
            .unwrap_or_else(update::default_channel)
    }

    /// The release to update to: the highest one the online servers of the kept rooms announced;
    /// `None` makes the check ask GitHub for its latest release.
    pub(super) fn release_target(&self) -> Option<semver::Version> {
        self.releases
            .iter()
            .filter(|(server, _)| self.rooms.has_server(server))
            .map(|(_, version)| version)
            .max()
            .cloned()
    }

    /// Checks again when the target moved; announcements steer only the release channel.
    pub(super) fn follow_release(&mut self, before: Option<semver::Version>) -> Task<Message> {
        if update::ENABLED && self.channel() == Channel::Release && self.release_target() != before
        {
            self.check_update(false)
        } else {
            Task::none()
        }
    }

    /// Looks up the newest dev build on the dev channel; on the release channel the release the
    /// servers announced, or GitHub's latest one when none did or `latest` asks for it. Not while
    /// a check or an install is under way, or an update waits for its restart.
    pub(super) fn check_update(&mut self, latest: bool) -> Task<Message> {
        if !update::ENABLED
            || matches!(
                self.updater,
                UpdateState::Checking | UpdateState::Installing(_) | UpdateState::Installed(_)
            )
        {
            return Task::none();
        }
        let Some(program) = &self.program else {
            self.updater = UpdateState::Failed("Cannot find this program's file".into());
            return Task::none();
        };
        let package = program.package;
        let channel = self.channel();
        match logic::next_check(
            &self.updater,
            channel,
            latest,
            self.release_target(),
            &schwitzer_release::version(),
        ) {
            Check::Skip => Task::none(),
            Check::UpToDate => {
                self.updater = UpdateState::UpToDate;
                Task::none()
            }
            Check::Ask(lookup) => {
                self.updater = UpdateState::Checking;
                Task::perform(update::check(lookup, package), move |result| {
                    Message::UpdateChecked {
                        latest,
                        channel,
                        result,
                    }
                })
            }
        }
    }

    /// The answer to a check for `channel`; dropped when the channel changed since, whose own
    /// check is under way.
    pub(super) fn update_checked(
        &mut self,
        latest: bool,
        channel: Channel,
        result: Result<Option<update::Release>, String>,
    ) -> Task<Message> {
        if self.updater != UpdateState::Checking || channel != self.channel() {
            return Task::none();
        }
        match result {
            Ok(None) => self.updater = UpdateState::UpToDate,
            Ok(Some(release))
                if !latest
                    && channel == Channel::Release
                    && self
                        .release_target()
                        .is_some_and(|target| target != release.version) =>
            {
                // The announcements moved while GitHub answered: look up the new target.
                self.updater = UpdateState::Idle;
                return self.check_update(false);
            }
            Ok(Some(release)) => {
                // A version that failed to install waits for the Install button.
                let error = self
                    .update_failed
                    .as_ref()
                    .filter(|(version, _)| *version == release.version)
                    .map(|(_, e)| e.clone());
                if error.is_none() && self.settings.auto_install_updates {
                    return self.install_update(release);
                }
                self.updater = UpdateState::Available { release, error };
            }
            // No banner: an offline machine would get one every five minutes.
            Err(e) => {
                tracing::warn!("update check failed: {e}");
                self.updater = UpdateState::Failed(e);
            }
        }
        Task::none()
    }

    pub(super) fn install_update(&mut self, release: update::Release) -> Task<Message> {
        let Some(program) = self.program.clone() else {
            return Task::none();
        };
        self.updater = UpdateState::Installing(release.clone());
        Task::perform(update::install(release, program), Message::UpdateInstalled)
    }

    /// Follows `channel` from now on and looks up its newest build at once; with auto-install
    /// that installs it, also an older one.
    pub(super) fn set_update_channel(&mut self, channel: Channel) -> Task<Message> {
        if self.settings.update_channel == Some(channel) {
            return Task::none();
        }
        self.settings.update_channel = Some(channel);
        self.save_settings();
        if matches!(
            self.updater,
            UpdateState::Checking
                | UpdateState::UpToDate
                | UpdateState::Failed(_)
                | UpdateState::Available { .. }
        ) {
            self.updater = UpdateState::Idle;
        }
        self.check_update(false)
    }

    pub(super) fn update_installed(
        &mut self,
        result: Result<semver::Version, String>,
    ) -> Task<Message> {
        let UpdateState::Installing(release) = &self.updater else {
            return Task::none();
        };
        let release = release.clone();
        match result {
            Ok(version) => {
                self.update_failed = None;
                self.updater = UpdateState::Installed(version.clone());
                if self.settings.auto_restart_updates {
                    return self.handle(Message::RestartForUpdate);
                }
                self.notify(&format!("Version {version} installed, restart to use it"));
            }
            Err(e) => {
                self.update_failed = Some((release.version.clone(), e.clone()));
                self.updater = UpdateState::Available {
                    release,
                    error: Some(e),
                };
            }
        }
        Task::none()
    }
}
