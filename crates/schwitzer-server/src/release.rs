//! The app release this server announces: the newest stable release on GitHub, whatever the
//! server's own version. With `--auto-update` the same check installs that release's server
//! (`update.rs`).

use std::sync::PoisonError;
use std::time::{Duration, Instant};

use schwitzer_proto::Event;
use schwitzer_release::Asset;
use serde::Deserialize;
use tokio::sync::oneshot;
use tokio::time::MissedTickBehavior;

use crate::update::AutoUpdate;
use crate::{State, USER_AGENT};

const CHECK_INTERVAL: Duration = Duration::from_secs(120);
const MAX_JSON: u64 = 4 << 20;
/// How long a version whose install failed is left alone before it is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(30 * 60);
/// Releases asked for per check; the newest stable one is among them.
const PER_PAGE: &str = "10";

#[derive(Deserialize)]
struct Listed {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// A stable release on GitHub and its downloads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stable {
    pub version: semver::Version,
    pub assets: Vec<Asset>,
}

/// The newest stable release in GitHub's release list `json`. Tags that are no version are
/// skipped.
pub(crate) fn newest_stable(json: &[u8]) -> Result<Option<Stable>, String> {
    let listed: Vec<Listed> =
        serde_json::from_slice(json).map_err(|e| format!("unreadable release list: {e}"))?;
    Ok(listed
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| {
            let version = schwitzer_release::parse_tag(&r.tag_name)?;
            Some(Stable {
                version,
                assets: r.assets,
            })
        })
        .filter(|s| schwitzer_release::is_stable(&s.version))
        .max_by(|a, b| a.version.cmp(&b.version)))
}

/// This build's version.
fn current() -> semver::Version {
    env!("CARGO_PKG_VERSION")
        .parse()
        .expect("CARGO_PKG_VERSION is semver")
}

fn fetch(url: &str) -> Result<Option<Stable>, String> {
    let json = schwitzer_release::api_get(
        USER_AGENT,
        Duration::from_secs(30),
        url,
        &[("per_page", PER_PAGE)],
        MAX_JSON,
    )?;
    newest_stable(&json)
}

/// Checks `url` every two minutes and announces a changed release to every stream. Stores the
/// release before publishing it: `/events` reads it after subscribing, so no stream misses it.
///
/// With `update`, a release newer than this server is installed over the program file and its
/// version sent on the channel; the task then idles until the server shuts down. A failed install
/// of a version is retried after [`RETRY_AFTER`].
pub(crate) async fn watch(
    state: State,
    url: String,
    mut update: Option<(AutoUpdate, oneshot::Sender<semver::Version>)>,
) {
    let mut interval = tokio::time::interval(CHECK_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut failed: Option<(semver::Version, Instant)> = None;
    loop {
        interval.tick().await;
        let u = url.clone();
        let found = match tokio::task::spawn_blocking(move || fetch(&u)).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!("release check stopped: {e}");
                continue;
            }
        };
        let stable = match found {
            Ok(Some(stable)) => stable,
            // No stable release listed: keep the last known release.
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("release check failed: {e}");
                continue;
            }
        };
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

        let Some((updater, _)) = &update else {
            continue;
        };
        let retry_later = failed
            .as_ref()
            .is_some_and(|(v, at)| *v == version && at.elapsed() < RETRY_AFTER);
        if version <= current() || retry_later {
            continue;
        }
        tracing::info!("installing server release {version}");
        let updater = updater.clone();
        let installed =
            tokio::task::spawn_blocking(move || updater.install(&stable.version, &stable.assets))
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
}
