//! The game project the editor works on.

use std::path::{Path, PathBuf};

use bevy_ecs::resource::Resource;

/// The scene the editor opens when it isn't told which, relative to the assets folder.
pub const DEFAULT_SCENE: &str = "scenes/main.scn.ron";

/// The game project the editor works on, and how to run it.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct EditorProject {
    /// The project's directory, which holds its `Cargo.toml` and `assets` folder.
    pub dir: PathBuf,
    /// The Cargo package Play runs, if the project has more than one.
    pub package: Option<String>,
    /// The `hearth` executable Play runs the game with.
    pub cli: PathBuf,
    /// The scene to open, relative to the assets folder.
    pub scene: PathBuf,
    /// Flags for `hearth run`, so Play builds the game the way the editor was built.
    pub run_args: Vec<String>,
}

impl EditorProject {
    /// A project in `dir`, opening the default scene and playing with `hearth` from `PATH`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            package: None,
            cli: PathBuf::from("hearth"),
            scene: PathBuf::from(DEFAULT_SCENE),
            run_args: Vec::new(),
        }
    }

    /// The project `hearth editor` describes in the environment:
    ///
    /// - `HEARTH_PROJECT`: the project directory, else `BEVY_ASSET_ROOT`, else the current one
    /// - `HEARTH_EDITOR_PACKAGE`: the package Play runs
    /// - `HEARTH_CLI`: the `hearth` executable, else `hearth` from `PATH`
    /// - `HEARTH_EDITOR_SCENE`: the scene to open, relative to the assets folder or absolute
    /// - `HEARTH_EDITOR_RUN_ARGS`: flags for `hearth run`, one per line
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        let dir = var("HEARTH_PROJECT")
            .or_else(|| var("BEVY_ASSET_ROOT"))
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let mut project = Self::new(dir);
        project.package = var("HEARTH_EDITOR_PACKAGE").and_then(|name| name.into_string().ok());
        if let Some(cli) = var("HEARTH_CLI") {
            project.cli = cli.into();
        }
        if let Some(scene) = var("HEARTH_EDITOR_SCENE") {
            project.scene = project.asset_relative(Path::new(&scene));
        }
        if let Some(args) = var("HEARTH_EDITOR_RUN_ARGS") {
            project.run_args = args
                .to_string_lossy()
                .lines()
                .map(str::trim)
                .filter(|arg| !arg.is_empty())
                .map(String::from)
                .collect();
        }
        project
    }

    /// The project's assets folder.
    pub fn assets(&self) -> PathBuf {
        self.dir.join("assets")
    }

    /// Where an asset path, relative to the assets folder, is on disk.
    pub fn asset_file(&self, path: &Path) -> PathBuf {
        self.assets().join(path)
    }

    /// `path` relative to the assets folder when it is inside it, else unchanged.
    pub fn asset_relative(&self, path: &Path) -> PathBuf {
        path.strip_prefix(self.assets())
            .or_else(|_| path.strip_prefix("assets"))
            .unwrap_or(path)
            .to_path_buf()
    }
}

/// An asset path as the asset server wants it: relative, with forward slashes.
pub fn asset_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_paths_are_kept_relative_to_assets() {
        let project = EditorProject::new("/game");
        assert_eq!(
            project.asset_relative(Path::new("/game/assets/scenes/a.scn.ron")),
            PathBuf::from("scenes/a.scn.ron")
        );
        assert_eq!(
            project.asset_relative(Path::new("assets/scenes/a.scn.ron")),
            PathBuf::from("scenes/a.scn.ron")
        );
        assert_eq!(
            project.asset_relative(Path::new("scenes/a.scn.ron")),
            PathBuf::from("scenes/a.scn.ron")
        );
        assert_eq!(
            asset_path_string(Path::new("scenes\\a.scn.ron")),
            "scenes/a.scn.ron"
        );
    }
}
