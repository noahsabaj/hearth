//! The asset browser: the files in the project's assets folder. Click a folder to open it, a
//! glTF model or scene file to add it to the scene, and a scene in `scenes/` to edit it.

use alloc::{borrow::ToOwned, format, string::String, vec::Vec};
use std::path::{Path, PathBuf};

use bevy_app::{App, Update};
use bevy_ecs::{
    entity::Entity,
    hierarchy::Children,
    observer::On,
    query::With,
    resource::Resource,
    system::{Commands, Res, ResMut},
    world::World,
};
use bevy_feathers::{
    controls::{ButtonVariant, FeathersButton},
    display::{label, label_dim},
};
use bevy_scene::{bsn, on};
use bevy_time::{Real, Time};
use bevy_ui::{px, widget::Text, FlexDirection, JustifyContent, Node};
use bevy_ui_widgets::Activate;

use crate::{
    layout::{self, AssetFolderLabel, AssetList},
    ops,
    project::{asset_path_string, EditorProject},
    scene,
    status::Status,
};

/// What kind of file an asset is, which decides what clicking it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetKind {
    /// A folder.
    Folder,
    /// A glTF model, added to the scene as an instance.
    Model,
    /// A scene file, opened for editing or added as an instance.
    Scene,
    /// An image.
    Image,
    /// A sound.
    Audio,
    /// Anything else.
    Other,
}

impl AssetKind {
    /// The kind of the file at `path`.
    pub fn of(path: &Path, is_dir: bool) -> Self {
        if is_dir {
            return Self::Folder;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let has = |suffixes: &[&str]| suffixes.iter().any(|suffix| name.ends_with(suffix));
        if has(&[".gltf", ".glb"]) {
            Self::Model
        } else if has(&[".scn.ron", ".scn"]) {
            Self::Scene
        } else if has(&[
            ".png", ".jpg", ".jpeg", ".ktx2", ".hdr", ".exr", ".dds", ".webp",
        ]) {
            Self::Image
        } else if has(&[".ogg", ".wav", ".mp3", ".flac"]) {
            Self::Audio
        } else {
            Self::Other
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Model => "model",
            Self::Scene => "scene",
            Self::Image => "image",
            Self::Audio => "sound",
            Self::Other => "file",
        }
    }
}

/// A file or folder in the asset browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetEntry {
    /// Its file name.
    pub name: String,
    /// Its path relative to the assets folder.
    pub path: PathBuf,
    /// What it is.
    pub kind: AssetKind,
}

/// The folder the asset browser shows, and what is in it.
#[derive(Resource, Debug, Default)]
pub struct AssetBrowser {
    /// The folder, relative to the assets folder.
    pub folder: PathBuf,
    /// Its folders and files, folders first, each sorted by name.
    pub entries: Vec<AssetEntry>,
    /// When it was last read, in seconds of real time.
    scanned_at: Option<f64>,
    /// Whether the list on screen shows `entries`.
    shown: bool,
}

impl AssetBrowser {
    /// Opens the folder above the current one.
    pub fn up(&mut self) {
        if self.folder.pop() {
            self.rescan_now();
        }
    }

    /// Opens `folder`, relative to the assets folder.
    pub fn open(&mut self, folder: PathBuf) {
        self.folder = folder;
        self.rescan_now();
    }

    /// Reads the folder again on the next frame.
    pub fn rescan_now(&mut self) {
        self.scanned_at = None;
    }
}

/// Lists the folders and files in `folder`, relative to `assets`.
pub fn read_folder(assets: &Path, folder: &Path) -> Vec<AssetEntry> {
    let Ok(read) = std::fs::read_dir(assets.join(folder)) else {
        return Vec::new();
    };
    let mut entries: Vec<AssetEntry> = read
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name.ends_with(".meta") {
                return None;
            }
            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
            let path = folder.join(&name);
            Some(AssetEntry {
                kind: AssetKind::of(&path, is_dir),
                name,
                path,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        (a.kind != AssetKind::Folder, a.name.to_lowercase())
            .cmp(&(b.kind != AssetKind::Folder, b.name.to_lowercase()))
    });
    entries
}

/// Reads the folder every couple of seconds, so new files show up.
fn rescan(mut browser: ResMut<AssetBrowser>, project: Res<EditorProject>, time: Res<Time<Real>>) {
    let now = time.elapsed_secs_f64();
    if browser
        .scanned_at
        .is_some_and(|scanned_at| now - scanned_at < 2.0)
    {
        return;
    }
    browser.scanned_at = Some(now);
    let entries = read_folder(&project.assets(), &browser.folder);
    if entries != browser.entries {
        browser.entries = entries;
        browser.shown = false;
    }
}

/// What clicking an asset does.
pub fn activate(world: &mut World, entry: &AssetEntry) {
    match entry.kind {
        AssetKind::Folder => world
            .resource_mut::<AssetBrowser>()
            .open(entry.path.clone()),
        AssetKind::Model => {
            ops::add_asset(world, &entry.path);
        }
        AssetKind::Scene => {
            let current = world.resource::<scene::EditedScene>().path.clone();
            if entry.path == current {
                Status::info(world, format!("{} is the scene being edited", entry.name));
                return;
            }
            let what = asset_path_string(&entry.path);
            if !layout::may_discard(world, &what) {
                return;
            }
            scene::open(world, entry.path.clone());
        }
        AssetKind::Image | AssetKind::Audio | AssetKind::Other => Status::info(
            world,
            format!(
                "{} is a {}; components that use it take its path, {}",
                entry.name,
                entry.kind.label(),
                asset_path_string(&entry.path)
            ),
        ),
    }
}

/// Shows the folder's entries when they change.
pub(crate) fn rebuild_asset_list(world: &mut World) {
    if world.resource::<AssetBrowser>().shown {
        return;
    }
    let Some(list) = world
        .query_filtered::<Entity, With<AssetList>>()
        .iter(world)
        .next()
    else {
        return;
    };
    world.resource_mut::<AssetBrowser>().shown = true;
    let (folder, entries) = {
        let browser = world.resource::<AssetBrowser>();
        (browser.folder.clone(), browser.entries.clone())
    };

    let folder_text = if folder.as_os_str().is_empty() {
        String::from("assets")
    } else {
        format!("assets/{}", asset_path_string(&folder))
    };
    let labels: Vec<Entity> = world
        .query_filtered::<Entity, With<AssetFolderLabel>>()
        .iter(world)
        .collect();
    for entity in labels {
        if let Some(mut text) = world.get_mut::<Text>(entity) {
            text.0 = folder_text.clone();
        }
    }

    let old: Vec<Entity> = world
        .get::<Children>(list)
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    for child in old {
        if let Ok(child) = world.get_entity_mut(child) {
            child.despawn();
        }
    }
    if entries.is_empty() {
        let message = if world.resource::<EditorProject>().assets().is_dir() {
            "This folder is empty"
        } else {
            "The project has no assets folder yet"
        };
        layout::spawn_into(world, list, bsn! { @label_dim(message) });
    }
    for entry in entries {
        let name = entry.name.clone();
        let kind = entry.kind.label().to_owned();
        let variant = if entry.kind == AssetKind::Folder {
            ButtonVariant::Normal
        } else {
            ButtonVariant::Plain
        };
        layout::spawn_into(
            world,
            list,
            bsn! {
                @FeathersButton {
                    @caption: bsn! {
                        Node {
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                        }
                        Children [
                            @label({name})
                            --
                            @label_dim({kind})
                        ]
                    },
                    @variant: {variant},
                }
                Node {
                    width: px(132),
                    height: px(44),
                    justify_content: JustifyContent::FlexStart,
                }
                on(move |_: On<Activate>, mut commands: Commands| {
                    let entry = entry.clone();
                    commands.queue(move |world: &mut World| activate(world, &entry));
                })
            },
        );
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<AssetBrowser>()
        .add_systems(Update, rescan);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_the_file_name() {
        assert_eq!(AssetKind::of(Path::new("a/b.GLB"), false), AssetKind::Model);
        assert_eq!(
            AssetKind::of(Path::new("main.scn.ron"), false),
            AssetKind::Scene
        );
        assert_eq!(AssetKind::of(Path::new("x.png"), false), AssetKind::Image);
        assert_eq!(AssetKind::of(Path::new("x"), true), AssetKind::Folder);
        assert_eq!(AssetKind::of(Path::new("x.toml"), false), AssetKind::Other);
    }

    #[test]
    fn folders_come_first() {
        let dir = std::env::temp_dir().join(format!("hearth-editor-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("models")).unwrap();
        std::fs::write(dir.join("a.png"), b"").unwrap();
        std::fs::write(dir.join(".hidden"), b"").unwrap();
        let entries = read_folder(&dir, Path::new(""));
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["models", "a.png"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
