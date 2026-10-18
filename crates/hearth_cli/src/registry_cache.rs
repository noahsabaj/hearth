//! Keeps each app's `registry.schema` on disk, so a command that needs the type registry asks
//! only for `app.info` while the copy still holds, instead of for the whole schema.
//!
//! A copy holds while the app reports the same run (`instance`) and type count. Apps that don't
//! report a run are never cached.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::Endpoint;

/// How long a copy is kept after it was last written.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Which run of an app, with how many types, a schema came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// The run, as `app.info` reports it.
    pub instance: String,
    /// How many types the app had registered.
    pub registered_types: usize,
}

impl Stamp {
    /// Reads the stamp from an `app.info` result, if the app reports its run.
    pub fn from_info(info: &Value) -> Option<Self> {
        let instance = info.get("instance")?.as_str()?;
        if instance.is_empty() {
            return None;
        }
        Some(Self {
            instance: instance.to_string(),
            registered_types: info.get("registered_types")?.as_u64()? as usize,
        })
    }
}

/// A copy of an app's schema.
#[derive(Deserialize)]
pub struct Cached {
    /// Where it came from.
    #[serde(flatten)]
    pub stamp: Stamp,
    /// The `registry.schema` result.
    pub schema: Value,
}

#[derive(Serialize)]
struct CachedRef<'a> {
    #[serde(flatten)]
    stamp: &'a Stamp,
    schema: &'a Value,
}

/// The directory copies are kept in.
pub fn dir() -> PathBuf {
    crate::session::home().join("registry")
}

/// Whether there is a copy for the app at `endpoint`.
pub fn exists(dir: &Path, endpoint: &Endpoint) -> bool {
    file(dir, endpoint).is_file()
}

/// The copy for the app at `endpoint`, if there is one.
pub fn load(dir: &Path, endpoint: &Endpoint) -> Option<Cached> {
    let bytes = fs::read(file(dir, endpoint)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Keeps `schema` as the copy for the app at `endpoint`, and drops copies nobody wrote for a
/// week. Failing to write only costs the next command a fetch, so errors are ignored.
pub fn store(dir: &Path, endpoint: &Endpoint, stamp: &Stamp, schema: &Value) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    prune(dir);
    let Ok(bytes) = serde_json::to_vec(&CachedRef { stamp, schema }) else {
        return;
    };
    // Write beside the copy and rename over it, so a reader never sees half a file.
    let path = file(dir, endpoint);
    let partial = path.with_extension(format!("{}.partial", std::process::id()));
    if fs::write(&partial, bytes).is_err() || fs::rename(&partial, &path).is_err() {
        let _ = fs::remove_file(&partial);
    }
}

fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() > KEEP);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn file(dir: &Path, endpoint: &Endpoint) -> PathBuf {
    let name: String = format!("{}_{}{}", endpoint.host, endpoint.port, endpoint.path)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{name}.json"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hearth-cli-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn stamp_needs_a_run() {
        assert_eq!(
            Stamp::from_info(&json!({ "instance": "7-1", "registered_types": 3 })),
            Some(Stamp {
                instance: "7-1".into(),
                registered_types: 3
            })
        );
        assert_eq!(
            Stamp::from_info(&json!({ "instance": "", "registered_types": 3 })),
            None
        );
        assert_eq!(Stamp::from_info(&json!({ "app_name": "Old app" })), None);
    }

    #[test]
    fn copies_round_trip_per_endpoint() {
        let dir = scratch("registry-cache");
        let here = Endpoint::localhost(15702);
        let there = Endpoint::new("example.com", 15702);
        let stamp = Stamp {
            instance: "7-1".into(),
            registered_types: 1,
        };
        let schema = json!({ "glam::Vec3": { "shortPath": "Vec3" } });
        assert!(load(&dir, &here).is_none());

        store(&dir, &here, &stamp, &schema);
        assert!(exists(&dir, &here));
        assert!(!exists(&dir, &there));
        let cached = load(&dir, &here).unwrap();
        assert_eq!(cached.stamp, stamp);
        assert_eq!(cached.schema, schema);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);

        fs::remove_dir_all(&dir).unwrap();
    }
}
