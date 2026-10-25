//! Writes assets/licenses/third-party.json, the open source components of the app and their
//! license texts, which the app's Licenses page shows: cargo run -p schwitzer-app --example
//! licenses (needs cargo-about 0.9.2). cargo-about finds the crates (per target, build
//! dependencies included) and their texts; this drops Schwitzer's own crates, links every
//! component to its source, joins equal texts and adds the NOTICE files of Apache-2.0 crates.
//! Its test fails when Cargo.lock moved on since the listing was written.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// What `cargo about --version` must print: another version may pick other texts, and the
/// committed listing would change with the machine.
const CARGO_ABOUT: &str = "cargo-about 0.9.2";

/// The listing, relative to the crate directory.
const LISTING: &str = "assets/licenses/third-party.json";

/// The listing as committed: the fingerprint of the lock file it was written from and one entry
/// per distinct license text.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Listing {
    lock: String,
    licenses: Vec<Entry>,
}

/// One license text and the components under it.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Entry {
    /// The SPDX id, or `NOTICE` for the NOTICE file of an Apache-2.0 crate.
    id: String,
    name: String,
    text: String,
    crates: Vec<Component>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd, Ord, Eq, Clone)]
struct Component {
    name: String,
    version: String,
    /// Where its source code is: its crates.io page, or the tree of the git revision.
    source: String,
}

/// The part of cargo-about's JSON output this example reads.
#[derive(Deserialize)]
struct About {
    licenses: Vec<AboutLicense>,
}

#[derive(Deserialize, Clone)]
struct AboutLicense {
    id: String,
    name: String,
    text: String,
    used_by: Vec<UsedBy>,
}

#[derive(Deserialize, Clone)]
struct UsedBy {
    #[serde(rename = "crate")]
    krate: AboutCrate,
}

#[derive(Deserialize, Clone)]
struct AboutCrate {
    name: String,
    version: String,
    /// `None` for a workspace crate.
    source: Option<String>,
    manifest_path: String,
}

/// The part of `Cargo.lock` the fingerprint covers.
#[derive(Deserialize)]
struct Lock {
    package: Vec<LockPackage>,
}

#[derive(Deserialize)]
struct LockPackage {
    name: String,
    version: String,
    source: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let version = Command::new("cargo")
        .args(["about", "--version"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    if version.as_deref() != Some(CARGO_ABOUT) {
        return Err("the licenses example needs cargo-about 0.9.2: cargo install --locked --features cli cargo-about@0.9.2".into());
    }
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir.join("../..");
    let output = Command::new("cargo")
        .current_dir(&root)
        .args(["about", "generate", "--frozen", "--fail", "-c"])
        .arg(crate_dir.join("about.toml"))
        .arg("-m")
        .arg(crate_dir.join("Cargo.toml"))
        .args(["--format", "json"])
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Err(format!("cargo about failed with {}", output.status).into());
    }
    let about: About = serde_json::from_slice(&output.stdout)?;
    let mut inputs = about.licenses;
    let notices = apache_notices(&inputs)?;
    inputs.extend(notices);
    let lock = lock_fingerprint(&fs::read_to_string(root.join("Cargo.lock"))?)?;
    let listing = merge(lock, inputs)?;
    let file = crate_dir.join(LISTING);
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&file, serde_json::to_string_pretty(&listing)? + "\n")?;
    let components: BTreeSet<_> = listing
        .licenses
        .iter()
        .flat_map(|e| &e.crates)
        .map(|c| (&c.name, &c.version))
        .collect();
    eprintln!(
        "{LISTING}: {} components, {} license texts",
        components.len(),
        listing.licenses.len()
    );
    Ok(())
}

/// SHA-256 over the packages of `Cargo.lock`: name, version and source of every third-party
/// crate and the dependencies of every package. Schwitzer's own crates (no `source`) count by
/// name only: `deploy/release.sh` bumps their version, which changes no component. Parsing the
/// lock file also makes the fingerprint independent of line endings.
fn lock_fingerprint(lock: &str) -> Result<String, Box<dyn Error>> {
    let lock: Lock = toml::from_str(lock)?;
    let mut hasher = Sha256::new();
    for package in &lock.package {
        match &package.source {
            Some(source) => hasher.update(format!("{} {} {source}", package.name, package.version)),
            None => hasher.update(&package.name),
        }
        for dependency in &package.dependencies {
            hasher.update(format!(" {dependency}"));
        }
        hasher.update(b"\n");
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Where the source of a crate from `source` (as in `Cargo.lock`) is.
fn source_url(name: &str, version: &str, source: &str) -> Result<String, Box<dyn Error>> {
    if source == "registry+https://github.com/rust-lang/crates.io-index"
        || source == "sparse+https://index.crates.io/"
    {
        return Ok(format!("https://crates.io/crates/{name}/{version}"));
    }
    Err(format!("{name} {version} comes from {source}: no source link for it").into())
}

/// Joins equal texts of the same license, drops Schwitzer's own crates and the texts only they
/// use, and sorts the result.
fn merge(lock: String, inputs: Vec<AboutLicense>) -> Result<Listing, Box<dyn Error>> {
    let mut groups: BTreeMap<(String, String), (String, BTreeSet<Component>)> = BTreeMap::new();
    for license in inputs {
        let group = groups
            .entry((license.id, license.text))
            .or_insert_with(|| (license.name, BTreeSet::new()));
        for used in license.used_by {
            let krate = used.krate;
            let Some(source) = krate.source else { continue };
            let source = source_url(&krate.name, &krate.version, &source)?;
            group.1.insert(Component {
                name: krate.name,
                version: krate.version,
                source,
            });
        }
    }
    let mut licenses: Vec<Entry> = groups
        .into_iter()
        .filter(|(_, (_, crates))| !crates.is_empty())
        .map(|((id, text), (name, crates))| Entry {
            id,
            name,
            text,
            crates: crates.into_iter().collect(),
        })
        .collect();
    licenses.sort_by(|a, b| {
        let first = |e: &Entry| {
            e.crates
                .first()
                .map(|c| (c.name.clone(), c.version.clone()))
        };
        (&a.id, first(a)).cmp(&(&b.id, first(b)))
    });
    Ok(Listing { lock, licenses })
}

/// The NOTICE files of the Apache-2.0 crates, which Apache-2.0 §4(d) requires to go with the
/// binary; cargo-about does not collect them.
fn apache_notices(inputs: &[AboutLicense]) -> Result<Vec<AboutLicense>, Box<dyn Error>> {
    let mut notices = Vec::new();
    let mut seen = BTreeSet::new();
    for license in inputs.iter().filter(|l| l.id.starts_with("Apache-2.0")) {
        for used in &license.used_by {
            let krate = &used.krate;
            if krate.source.is_none() || !seen.insert(krate.manifest_path.clone()) {
                continue;
            }
            let Some(dir) = Path::new(&krate.manifest_path).parent() else {
                continue;
            };
            let Some(file) = ["NOTICE", "NOTICE.txt", "NOTICE.md"]
                .into_iter()
                .map(|name| dir.join(name))
                .find(|file| file.is_file())
            else {
                continue;
            };
            notices.push(AboutLicense {
                id: "NOTICE".to_owned(),
                name: format!("NOTICE of {} {}", krate.name, krate.version),
                text: fs::read_to_string(&file)
                    .map_err(|e| format!("read {}: {e}", file.display()))?,
                used_by: vec![used.clone()],
            });
        }
    }
    Ok(notices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_was_written_from_the_current_lock_file() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let lock = fs::read_to_string(crate_dir.join("../../Cargo.lock")).unwrap();
        let json = fs::read_to_string(crate_dir.join(LISTING)).unwrap();
        let listing: Listing = serde_json::from_str(&json).unwrap();
        assert_eq!(
            listing.lock,
            lock_fingerprint(&lock).unwrap(),
            "assets/licenses/third-party.json was written from another Cargo.lock: run `cargo run -p schwitzer-app --example licenses` (needs cargo-about 0.9.2)"
        );
    }

    const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

    fn used(name: &str, version: &str, source: Option<&str>) -> UsedBy {
        UsedBy {
            krate: AboutCrate {
                name: name.to_owned(),
                version: version.to_owned(),
                source: source.map(str::to_owned),
                manifest_path: format!("/nowhere/{name}/Cargo.toml"),
            },
        }
    }

    fn license(id: &str, text: &str, used_by: Vec<UsedBy>) -> AboutLicense {
        AboutLicense {
            id: id.to_owned(),
            name: format!("{id} License"),
            text: text.to_owned(),
            used_by,
        }
    }

    fn component(name: &str, version: &str, source: &str) -> Component {
        Component {
            name: name.to_owned(),
            version: version.to_owned(),
            source: source.to_owned(),
        }
    }

    #[test]
    fn merge_joins_equal_texts_drops_own_crates_and_links_sources() {
        let inputs = vec![
            license(
                "MIT",
                "A",
                vec![
                    used("foo", "1.0.0", Some(CRATES_IO)),
                    used("schwitzer-x", "0.1.0", None),
                ],
            ),
            license(
                "MIT",
                "A",
                vec![
                    used("bar", "2.0.0", Some(CRATES_IO)),
                    used("foo", "1.0.0", Some(CRATES_IO)),
                ],
            ),
            license(
                "Apache-2.0",
                "B",
                vec![used("weida", "0.1.0-alpha.9", Some(CRATES_IO))],
            ),
            license(
                "MIT OR Apache-2.0",
                "C",
                vec![used("schwitzer-y", "0.1.0", None)],
            ),
        ];
        let listing = merge("h".to_owned(), inputs).unwrap();
        assert_eq!(
            listing,
            Listing {
                lock: "h".to_owned(),
                licenses: vec![
                    Entry {
                        id: "Apache-2.0".to_owned(),
                        name: "Apache-2.0 License".to_owned(),
                        text: "B".to_owned(),
                        crates: vec![component(
                            "weida",
                            "0.1.0-alpha.9",
                            "https://crates.io/crates/weida/0.1.0-alpha.9"
                        )],
                    },
                    Entry {
                        id: "MIT".to_owned(),
                        name: "MIT License".to_owned(),
                        text: "A".to_owned(),
                        crates: vec![
                            component("bar", "2.0.0", "https://crates.io/crates/bar/2.0.0"),
                            component("foo", "1.0.0", "https://crates.io/crates/foo/1.0.0"),
                        ],
                    },
                ],
            }
        );
    }

    #[test]
    fn a_crate_from_an_unknown_source_is_an_error() {
        let inputs = vec![license(
            "MIT",
            "A",
            vec![used(
                "foo",
                "1.0.0",
                Some("registry+https://example.org/index"),
            )],
        )];
        assert!(merge("h".to_owned(), inputs).is_err());
    }

    #[test]
    fn a_crate_from_git_is_an_error() {
        let inputs = vec![license(
            "MIT",
            "A",
            vec![used(
                "foo",
                "1.0.0",
                Some("git+https://example.org/foo.git?rev=abc#abc"),
            )],
        )];
        assert!(merge("h".to_owned(), inputs).is_err());
    }

    fn two_packages(own_version: &str, foo_version: &str, own_dependencies: &str) -> String {
        format!(
            "version = 4\n\n[[package]]\nname = \"schwitzer-app\"\nversion = \"{own_version}\"\ndependencies = [{own_dependencies}]\n\n[[package]]\nname = \"foo\"\nversion = \"{foo_version}\"\nsource = \"{CRATES_IO}\"\n"
        )
    }

    #[test]
    fn the_fingerprint_ignores_own_versions_but_not_third_party_ones() {
        let fingerprint = |lock: String| lock_fingerprint(&lock).unwrap();
        let base = fingerprint(two_packages("0.1.5", "1.0.0", "\"foo\""));
        assert_eq!(base, fingerprint(two_packages("0.1.6", "1.0.0", "\"foo\"")));
        assert_ne!(base, fingerprint(two_packages("0.1.5", "1.0.1", "\"foo\"")));
        assert_ne!(
            base,
            fingerprint(two_packages("0.1.5", "1.0.0", "\"foo\", \"bar\""))
        );
    }
}
