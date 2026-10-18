//! The app release this server announces: the newest stable release on GitHub, whatever the
//! server's own version.

use std::sync::PoisonError;
use std::time::Duration;

use schwitzer_proto::Event;
use serde::Deserialize;
use tokio::time::MissedTickBehavior;

use crate::State;

/// GitHub API URL of the schwitzer release list.
pub const RELEASES_URL: &str = "https://api.github.com/repos/griasdi/schwitzer/releases";
const CHECK_INTERVAL: Duration = Duration::from_secs(300);
const MAX_JSON: u64 = 4 << 20;
/// Releases asked for per check; the newest stable one is among them.
const PER_PAGE: &str = "10";

#[derive(Deserialize)]
struct Listed {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// The newest stable release in GitHub's release list `json`. Tags that are no version are
/// skipped.
pub(crate) fn newest_stable(json: &[u8]) -> Result<Option<semver::Version>, String> {
    let listed: Vec<Listed> =
        serde_json::from_slice(json).map_err(|e| format!("unreadable release list: {e}"))?;
    Ok(listed
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| {
            let tag = r.tag_name.as_str();
            tag.strip_prefix('v')
                .unwrap_or(tag)
                .parse::<semver::Version>()
                .ok()
        })
        .filter(|v| v.pre.is_empty())
        .max())
}

fn fetch(url: &str) -> Result<Option<semver::Version>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .user_agent(format!("schwitzer-server/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .query("per_page", PER_PAGE)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| format!("GitHub: {e}"))?;
    let json = response
        .body_mut()
        .with_config()
        .limit(MAX_JSON)
        .read_to_vec()
        .map_err(|e| format!("GitHub: {e}"))?;
    newest_stable(&json)
}

/// Checks `url` every five minutes and announces a changed release to every stream. Stores the
/// release before publishing it: `/events` reads it after subscribing, so no stream misses it.
pub(crate) async fn watch(state: State, url: String) {
    let mut interval = tokio::time::interval(CHECK_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
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
        let version = match found {
            Ok(Some(version)) => version,
            // No stable release listed: keep the last known release.
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("release check failed: {e}");
                continue;
            }
        };
        {
            let mut known = state.release.lock().unwrap_or_else(PoisonError::into_inner);
            if known.as_ref() == Some(&version) {
                continue;
            }
            *known = Some(version.clone());
        }
        tracing::info!("announcing release {version}");
        state.hub.publish_all(&Event::Release {
            version: version.to_string(),
        });
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
        assert_eq!(newest_stable(&json), Ok(Some(v("0.3.0"))));
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
        assert_eq!(newest_stable(&json), Ok(Some(v("0.1.0"))));
    }

    #[test]
    fn no_release_or_a_bad_list() {
        assert_eq!(newest_stable(b"[]"), Ok(None));
        assert!(newest_stable(b"not json").is_err());
    }
}
