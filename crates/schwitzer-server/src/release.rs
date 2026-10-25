//! The app release this server announces: the newest stable release on GitHub, whatever the
//! server's own version. With `--auto-update` the same check installs the server of that release,
//! or of the newest build on the rolling `dev` prerelease with `--update-channel dev`
//! (`update.rs`).

use std::sync::PoisonError;
use std::time::{Duration, Instant};

use schwitzer_proto::Event;
use schwitzer_release::{Asset, Channel, GitHubRelease};
use tokio::sync::oneshot;
use tokio::time::MissedTickBehavior;

use crate::update::AutoUpdate;
use crate::{State, user_agent};

const CHECK_INTERVAL: Duration = Duration::from_secs(120);
const MAX_JSON: u64 = 4 << 20;
/// How long a version whose install failed is left alone before it is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(30 * 60);
/// Releases asked for per check; the newest stable one is among them.
const PER_PAGE: &str = "10";

/// A published server build on GitHub, a stable release or a dev build, and the downloads of
/// its release.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Published {
    pub version: semver::Version,
    pub assets: Vec<Asset>,
}

/// The newest stable release in GitHub's release list `json`. Tags that are no version are
/// skipped.
pub(crate) fn newest_stable(json: &[u8]) -> Result<Option<Published>, String> {
    let listed: Vec<GitHubRelease> =
        serde_json::from_slice(json).map_err(|e| format!("unreadable release list: {e}"))?;
    Ok(listed
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| {
            let version = schwitzer_release::parse_tag(&r.tag_name)?;
            Some(Published {
                version,
                assets: r.assets,
            })
        })
        .filter(|s| schwitzer_release::is_stable(&s.version))
        .max_by(|a, b| a.version.cmp(&b.version)))
}

/// The newest server build for `platform` on the dev release `json` (one release), with all of
/// the release's assets.
pub(crate) fn newest_dev(json: &[u8], platform: &str) -> Result<Option<Published>, String> {
    let dev: GitHubRelease =
        serde_json::from_slice(json).map_err(|e| format!("unreadable release: {e}"))?;
    if dev.draft {
        return Ok(None);
    }
    let version = schwitzer_release::newest_asset(
        &dev.assets,
        "schwitzer-server-",
        &format!("-{platform}.tar.gz"),
    )
    .map(|(version, _)| version);
    Ok(version.map(|version| Published {
        version,
        assets: dev.assets,
    }))
}

fn fetch(url: &str) -> Result<Option<Published>, String> {
    let json = schwitzer_release::api_get(&user_agent(), url, &[("per_page", PER_PAGE)], MAX_JSON)?;
    newest_stable(&json)
}

fn fetch_dev(url: &str, platform: &str) -> Result<Option<Published>, String> {
    let json = schwitzer_release::api_get_dev(&user_agent(), url, MAX_JSON)?;
    newest_dev(&json, platform)
}

/// Checks `url` every two minutes and announces a changed release to every stream. Stores the
/// release before publishing it: `/events` reads it after subscribing, so no stream misses it.
///
/// With `update`, the newest build of its channel is installed over the program file when this
/// server installs it ([`schwitzer_release::is_update`]), and its version sent on the channel;
/// the task then idles until the server shuts down. The dev release is asked every third check
/// only, which keeps a dev server below GitHub's unauthenticated limit. A failed install of a
/// version is retried after [`RETRY_AFTER`].
pub(crate) async fn watch(
    state: State,
    url: String,
    mut update: Option<(AutoUpdate, oneshot::Sender<semver::Version>)>,
) {
    let mut interval = tokio::time::interval(CHECK_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut failed: Option<(semver::Version, Instant)> = None;
    let mut ticks: u32 = 0;
    loop {
        interval.tick().await;
        let tick = ticks;
        ticks = ticks.wrapping_add(1);
        let u = url.clone();
        let stable = match tokio::task::spawn_blocking(move || fetch(&u)).await {
            Ok(Ok(Some(stable))) => Some(stable),
            // No stable release listed: keep the last known release.
            Ok(Ok(None)) => None,
            Ok(Err(e)) => {
                tracing::warn!("release check failed: {e}");
                None
            }
            Err(e) => {
                tracing::warn!("release check stopped: {e}");
                None
            }
        };
        if let Some(stable) = &stable {
            let version = stable.version.clone();
            let changed = {
                let mut known = state.release.lock().unwrap_or_else(PoisonError::into_inner);
                let changed = known.as_ref() != Some(&version);
                if changed {
                    *known = Some(version.clone());
                }
                changed
            };
            if changed {
                tracing::info!("announcing release {version}");
                state.hub.publish_all(&Event::Release {
                    version: version.to_string(),
                });
            }
        }

        let Some((updater, _)) = &update else {
            continue;
        };
        let target = match updater.channel() {
            Channel::Release => stable,
            Channel::Dev if tick.is_multiple_of(3) => {
                let (u, platform) = (url.clone(), updater.platform());
                match tokio::task::spawn_blocking(move || fetch_dev(&u, platform)).await {
                    Ok(Ok(found)) => found,
                    Ok(Err(e)) => {
                        tracing::warn!("dev release check failed: {e}");
                        None
                    }
                    Err(e) => {
                        tracing::warn!("dev release check stopped: {e}");
                        None
                    }
                }
            }
            Channel::Dev => None,
        };
        let Some(target) = target else {
            continue;
        };
        let version = target.version.clone();
        let retry_later = failed
            .as_ref()
            .is_some_and(|(v, at)| *v == version && at.elapsed() < RETRY_AFTER);
        if !schwitzer_release::is_update(&schwitzer_release::version(), &version) || retry_later {
            continue;
        }
        tracing::info!("installing server release {version}");
        let updater = updater.clone();
        let installed =
            tokio::task::spawn_blocking(move || updater.install(&target.version, &target.assets))
                .await
                .unwrap_or_else(|e| Err(format!("update worker stopped: {e}")));
        match installed {
            Ok(()) => {
                if let Some((_, tx)) = update.take() {
                    let _ = tx.send(version);
                }
                // Ending would read as a failed task in `Server::stopped`.
                std::future::pending::<()>().await;
            }
            Err(e) => {
                tracing::warn!("server update to {version} failed: {e}");
                failed = Some((version, Instant::now()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(releases: &[(&str, bool, bool)]) -> Vec<u8> {
        let items: Vec<_> = releases
            .iter()
            .map(|(tag, draft, prerelease)| {
                serde_json::json!({ "tag_name": tag, "draft": draft, "prerelease": prerelease })
            })
            .collect();
        serde_json::to_vec(&items).unwrap()
    }

    fn v(s: &str) -> semver::Version {
        s.parse().unwrap()
    }

    #[test]
    fn the_newest_stable_release_wins() {
        let json = list(&[
            ("v0.3.0", false, false),
            ("v0.2.0", false, false),
            ("v0.1.5", false, false),
        ]);
        assert_eq!(
            newest_stable(&json).map(|s| s.map(|s| s.version)),
            Ok(Some(v("0.3.0")))
        );
    }

    #[test]
    fn the_winner_keeps_its_assets() {
        let json = serde_json::to_vec(&serde_json::json!([
            { "tag_name": "v0.1.0", "assets": [] },
            { "tag_name": "v0.2.0", "assets": [{
                "name": "schwitzer-server-0.2.0-linux-x86_64.tar.gz",
                "browser_download_url": "https://example.org/s.tar.gz",
                "size": 42,
                "digest": "sha256:00"
            }] },
        ]))
        .unwrap();
        let stable = newest_stable(&json).unwrap().unwrap();
        assert_eq!(stable.version, v("0.2.0"));
        assert_eq!(stable.assets.len(), 1);
        assert_eq!(
            stable.assets[0].name,
            "schwitzer-server-0.2.0-linux-x86_64.tar.gz"
        );
        assert_eq!(stable.assets[0].size, 42);
    }

    #[test]
    fn drafts_prereleases_and_odd_tags_are_skipped() {
        let json = list(&[
            ("v0.1.9", true, false),
            ("v0.1.8", false, true),
            ("v0.1.7-rc.1", false, false),
            ("nightly", false, false),
            ("v0.1.0", false, false),
        ]);
        assert_eq!(
            newest_stable(&json).map(|s| s.map(|s| s.version)),
            Ok(Some(v("0.1.0")))
        );
    }

    #[test]
    fn no_release_or_a_bad_list() {
        assert_eq!(newest_stable(b"[]"), Ok(None));
        assert!(newest_stable(b"not json").is_err());
    }

    fn dev(names: &[&str]) -> Vec<u8> {
        let assets: Vec<_> = names
            .iter()
            .map(|name| {
                serde_json::json!({
                    "name": name,
                    "browser_download_url": format!("https://example.org/{name}"),
                    "size": 1,
                    "digest": null
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "tag_name": "dev",
            "draft": false,
            "prerelease": true,
            "assets": assets
        }))
        .unwrap()
    }

    #[test]
    fn the_newest_dev_server_for_the_platform_wins() {
        let json = dev(&[
            "schwitzer-server-0.3.1-dev.9-linux-x86_64.tar.gz",
            "schwitzer-server-0.3.1-dev.12-linux-x86_64.tar.gz",
            "schwitzer-server-0.3.1-dev.13-linux-aarch64.tar.gz",
            "schwitzer-0.3.1-dev.14-linux-x86_64.tar.gz",
        ]);
        let found = newest_dev(&json, "linux-x86_64").unwrap().unwrap();
        assert_eq!(found.version, v("0.3.1-dev.12"));
        // All of the release's assets: the install picks the archive by name again.
        assert_eq!(found.assets.len(), 4);
    }

    #[test]
    fn a_dev_release_without_a_server_for_the_platform_is_none() {
        let json = dev(&[
            "schwitzer-server-0.3.1-dev.13-linux-aarch64.tar.gz",
            "schwitzer-0.3.1-dev.14-linux-x86_64.tar.gz",
        ]);
        assert_eq!(newest_dev(&json, "linux-x86_64"), Ok(None));
    }
}
