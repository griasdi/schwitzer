//! The licenses the Licenses page shows: Schwitzer's own (MIT or Apache-2.0), those of the
//! bundled Figtree font and Lucide icons, and those of the open source components the app is
//! built from (`assets/licenses/third-party.json`, written by
//! `cargo run -p schwitzer-app --example licenses`).

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::LazyLock;

use serde::Deserialize;

pub const COPYRIGHT: &str = "Copyright (c) 2026 Felix Itzenplitz";

const MIT: &str = include_str!("../../../LICENSE-MIT");
const APACHE: &str = include_str!("../../../LICENSE-APACHE");
const FIGTREE: &str = include_str!("../assets/fonts/OFL.txt");
const LUCIDE: &str = include_str!("../assets/icons/LICENSE");
const THIRD_PARTY: &str = include_str!("../assets/licenses/third-party.json");

/// Which block of the Licenses page an entry belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Schwitzer,
    Assets,
    Components,
}

/// One license text and what it covers.
pub struct Entry {
    pub group: Group,
    pub title: Cow<'static, str>,
    /// What falls under it: the license's name for an asset, the components for a component
    /// license.
    pub caption: Option<String>,
    /// One line per component, `name version  source`; empty for Schwitzer's own and the assets.
    pub sources: Vec<String>,
    pub text: Cow<'static, str>,
}

/// The part of the listing the app reads.
#[derive(Deserialize)]
struct Listing {
    licenses: Vec<ListedLicense>,
}

#[derive(Deserialize)]
struct ListedLicense {
    name: String,
    text: String,
    crates: Vec<Component>,
}

#[derive(Deserialize)]
struct Component {
    name: String,
    version: String,
    source: String,
}

/// Every entry in page order; the Licenses page names its rows by the index.
static ENTRIES: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    let own = |group, title: &'static str, caption: Option<&str>, text: &'static str| Entry {
        group,
        title: Cow::Borrowed(title),
        caption: caption.map(str::to_owned),
        sources: Vec::new(),
        text: Cow::Borrowed(text),
    };
    let mut entries = vec![
        own(Group::Schwitzer, "MIT License", None, MIT),
        own(Group::Schwitzer, "Apache License 2.0", None, APACHE),
        own(
            Group::Assets,
            "Figtree",
            Some("SIL Open Font License 1.1"),
            FIGTREE,
        ),
        own(Group::Assets, "Lucide icons", Some("ISC License"), LUCIDE),
    ];
    let listing: Listing = serde_json::from_str(THIRD_PARTY)
        .expect("assets/licenses/third-party.json is written by the licenses example");
    entries.extend(listing.licenses.into_iter().map(|license| {
        let caption = license
            .crates
            .iter()
            .map(|c| format!("{} {}", c.name, c.version))
            .collect::<Vec<_>>()
            .join(", ");
        let sources = license
            .crates
            .iter()
            .map(|c| format!("{} {}  {}", c.name, c.version, c.source))
            .collect();
        Entry {
            group: Group::Components,
            title: Cow::Owned(license.name),
            caption: Some(caption),
            sources,
            text: Cow::Owned(license.text),
        }
    }));
    entries
});

/// The number of distinct open source components in the listing: a component under several
/// licenses has the same source line under each.
static COMPONENTS: LazyLock<usize> = LazyLock::new(|| {
    ENTRIES
        .iter()
        .flat_map(|e| &e.sources)
        .collect::<HashSet<_>>()
        .len()
});

pub fn entries() -> &'static [Entry] {
    &ENTRIES
}

pub fn component_count() -> usize {
    *COMPONENTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_names_the_apps_dependencies() {
        let captions: Vec<&str> = entries()
            .iter()
            .filter(|e| e.group == Group::Components)
            .filter_map(|e| e.caption.as_deref())
            .collect();
        assert!(captions.iter().any(|c| c.contains("iced ")));
        assert!(captions.iter().any(|c| c.contains("weida ")));
        assert!(!captions.iter().any(|c| c.contains("schwitzer-")));
    }
}
