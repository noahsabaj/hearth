//! `hearth editor`: the Hearth editor, built as a small crate that links the project's game code,
//! so the editor knows the game's reflected types.
//!
//! The crate is generated in the project's target directory. It depends on the engine from the
//! same source as the project, since a path and a git copy of the engine are different crates
//! whose types don't match, and it builds into the project's target directory with the project's
//! `Cargo.lock` and profiles, so it shares the engine build with `hearth run`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::{json, Value};

use super::project::{
    built_executable, free_port, launch_attached, launch_detached, prepare_session, Detached,
    Launch,
};
use super::screenshot::display_path;
use crate::cli::EditorArgs;
use crate::context::Ctx;
use crate::output::{CliError, Exit, Outcome};
use crate::project::{self, run_env, select_package, Package};

/// The editor crate's package and binary name, and its directory in the target directory.
const EDITOR: &str = "hearth-editor";

/// The engine feature that has the editor.
const EDITOR_FEATURE: &str = "hearth_editor";

/// The default session name of a detached editor.
const SESSION: &str = "editor";

/// Where the engine comes from, in the terms a `Cargo.toml` uses.
#[derive(Clone, Debug, PartialEq, Eq)]
enum EngineSource {
    /// A local checkout.
    Path(PathBuf),
    /// A git repository, at a `branch`, `tag` or `rev` when one is given.
    Git {
        /// The repository.
        url: String,
        /// The kind of reference and its value, such as `("branch", "main")`.
        reference: Option<(String, String)>,
    },
    /// A registry, at a version requirement.
    Registry {
        /// The version requirement, such as `^0.19`.
        version: String,
        /// The registry's index, when it isn't crates.io.
        index: Option<String>,
    },
}

impl EngineSource {
    /// The dependency as an inline TOML table, enabling `features`.
    fn toml(&self, features: &[&str]) -> String {
        let mut fields = match self {
            Self::Path(path) => vec![format!("path = {}", toml_path(path))],
            Self::Git { url, reference } => {
                let mut fields = vec![format!("git = {}", toml_string(url))];
                if let Some((kind, value)) = reference {
                    fields.push(format!("{kind} = {}", toml_string(value)));
                }
                fields
            }
            Self::Registry { version, .. } => vec![format!("version = {}", toml_string(version))],
        };
        if !features.is_empty() {
            fields.push(format!("features = {}", toml_list(features)));
        }
        format!("{{ {} }}", fields.join(", "))
    }
}

/// A TOML basic string.
fn toml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A path as a TOML string, with forward slashes, which Windows accepts too.
fn toml_path(path: &Path) -> String {
    toml_string(&display_path(path).replace('\\', "/"))
}

/// A TOML array of strings.
fn toml_list<S: AsRef<str>>(items: &[S]) -> String {
    let items: Vec<String> = items
        .iter()
        .map(|item| toml_string(item.as_ref()))
        .collect();
    format!("[{}]", items.join(", "))
}

/// Decodes the `%XX` escapes cargo writes in the query of a git source.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses a git source such as `git+https://github.com/noahsabaj/hearth?branch=main`, as a
/// dependency declares it, or with the `#commit` a resolved package adds.
fn git_source(source: &str) -> Option<EngineSource> {
    let rest = source.strip_prefix("git+")?;
    let rest = rest.split('#').next().unwrap_or(rest);
    let (url, query) = match rest.split_once('?') {
        Some((url, query)) => (url, Some(query)),
        None => (rest, None),
    };
    let reference = query.and_then(|query| {
        query.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            matches!(key, "branch" | "tag" | "rev")
                .then(|| (key.to_string(), percent_decode(value)))
        })
    });
    Some(EngineSource::Git {
        url: url.to_string(),
        reference,
    })
}

/// Where the project gets the engine: its `hearth` dependency, else the `hearth` package cargo
/// resolved, which the project may get through another crate. In the engine's own repository,
/// the engine is the project.
fn engine_source(package: &Package, all: &[Package]) -> Result<EngineSource, CliError> {
    if package.name == "hearth" {
        return Ok(EngineSource::Path(package.dir().to_path_buf()));
    }
    let declared = package
        .dependencies
        .iter()
        .filter(|dependency| dependency.name == "hearth")
        // A normal dependency before a build or dev dependency.
        .min_by_key(|dependency| dependency.kind.is_some());
    if let Some(dependency) = declared {
        if let Some(path) = &dependency.path {
            return Ok(EngineSource::Path(path.clone()));
        }
        match dependency.source.as_deref() {
            Some(source) if source.starts_with("git+") => {
                if let Some(git) = git_source(source) {
                    return Ok(git);
                }
            }
            Some(source) if source.starts_with("registry+") || source.starts_with("sparse+") => {
                return Ok(EngineSource::Registry {
                    version: dependency.req.clone(),
                    index: dependency.registry.clone(),
                });
            }
            // Cargo before 1.51 doesn't report where a path dependency is.
            _ => {}
        }
    }
    let resolved = all.iter().find(|candidate| candidate.name == "hearth");
    match resolved.map(|hearth| (hearth, hearth.source.as_deref())) {
        Some((hearth, None)) => Ok(EngineSource::Path(hearth.dir().to_path_buf())),
        Some((_, Some(source))) if source.starts_with("git+") => git_source(source)
            .ok_or_else(|| CliError::internal(format!("unexpected git source `{source}`"))),
        Some((hearth, Some(_))) => Ok(EngineSource::Registry {
            version: format!("={}", hearth.version),
            index: None,
        }),
        None => Err(CliError::new(
            "NO_ENGINE_DEPENDENCY",
            Exit::Usage,
            format!("`{}` doesn't depend on the engine (`hearth`)", package.name),
        )
        .hint("The editor builds with the project's engine: add a `hearth` dependency, or pick the game's package with -p.")),
    }
}

/// The name the library of `package` is used by in Rust code, if the package has one the
/// editor can link.
fn library_crate(package: &Package) -> Option<String> {
    package
        .targets
        .iter()
        .find(|target| target.is("lib") || target.is("rlib") || target.is("dylib"))
        .map(|target| target.name.replace('-', "_"))
}

/// Whether `word` can be a segment of a Rust path to a function.
fn is_identifier(word: &str) -> bool {
    let word = word.strip_prefix("r#").unwrap_or(word);
    let mut chars = word.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(word, "_" | "crate" | "self" | "super")
}

/// The function of the library that `[package.metadata.hearth] editor-hook` names, as a path
/// inside the library such as `register_types` or `editor::setup`. A leading `crate::` is
/// dropped.
fn editor_hook(metadata: &Value) -> Result<Option<String>, CliError> {
    let value = &metadata["hearth"]["editor-hook"];
    if value.is_null() {
        return Ok(None);
    }
    let text = value.as_str().unwrap_or_default().trim();
    let path = text.strip_prefix("crate::").unwrap_or(text);
    if path.is_empty() || !path.split("::").all(is_identifier) {
        return Err(CliError::new(
            "INVALID_EDITOR_HOOK",
            Exit::Usage,
            format!("`editor-hook = {value}` in `[package.metadata.hearth]` isn't the path of a function"),
        )
        .hint("Name a function of the library that takes `&mut App`, such as `editor-hook = \"register_types\"`."));
    }
    Ok(Some(path.to_string()))
}

/// The name of a TOML table header such as `[profile.dev]`, without its brackets. Arrays of
/// tables, `[[bin]]`, have none.
fn table_name(line: &str) -> Option<&str> {
    let inner = line.trim().strip_prefix('[')?;
    if inner.starts_with('[') {
        return None;
    }
    Some(inner.split(']').next()?.trim())
}

/// The `[profile]` tables of a `Cargo.toml`, without comments and blank lines. The editor
/// builds with them so the engine builds the same way as for the project, which shares the
/// build. Profiles only apply from a workspace's root manifest.
fn profile_tables(manifest: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            let name = table_name(trimmed);
            inside = name.is_some_and(|name| name == "profile" || name.starts_with("profile."));
            if let Some(name) = name.filter(|_| inside) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("[{name}]\n"));
            }
            continue;
        }
        if inside && !trimmed.is_empty() && !trimmed.starts_with('#') {
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    out
}

/// Whether a `Cargo.toml` has `[patch]` or `[replace]` tables, which the editor's crate doesn't
/// apply.
fn has_patches(manifest: &str) -> bool {
    manifest
        .lines()
        .filter_map(table_name)
        .any(|name| name == "patch" || name.starts_with("patch.") || name == "replace")
}

/// The project's library, linked into the editor.
#[derive(Clone, Debug)]
struct Library {
    /// The name Rust code uses for it.
    crate_name: String,
    /// Its package.
    package: String,
    /// The package's directory.
    dir: PathBuf,
    /// Features of the package to enable, besides its default ones.
    features: Vec<String>,
    /// When set, the package's default features are off, and these are on in their place.
    defaults: Option<Vec<String>>,
}

impl Library {
    /// The dependency as an inline TOML table.
    fn toml(&self) -> String {
        let mut fields = vec![
            format!("path = {}", toml_path(&self.dir)),
            format!("package = {}", toml_string(&self.package)),
        ];
        let mut features = self.features.clone();
        if let Some(defaults) = &self.defaults {
            fields.push("default-features = false".to_string());
            features.splice(0..0, defaults.iter().cloned());
        }
        if !features.is_empty() {
            fields.push(format!("features = {}", toml_list(&features)));
        }
        format!("{{ {} }}", fields.join(", "))
    }
}

/// The package's default features without those that link the engine dynamically, or `None` when
/// none of them does.
///
/// The editor links the engine statically. Dynamically linked, the engine with the editor in it
/// exports more symbols than a Windows DLL can hold (65,535). The game, when Play runs it, still
/// links it dynamically, and both builds share the engine's compiled crates.
fn static_defaults(features: &Value) -> Option<Vec<String>> {
    let defaults = project::strings(&features["default"]);
    let kept: Vec<String> = defaults
        .iter()
        .filter(|feature| !links_dynamically(features, feature, &mut Vec::new()))
        .cloned()
        .collect();
    (kept.len() != defaults.len()).then_some(kept)
}

/// Whether `feature` turns on a dependency's `dynamic_linking`, directly or through the package's
/// other features.
fn links_dynamically(features: &Value, feature: &str, seen: &mut Vec<String>) -> bool {
    if feature.ends_with("/dynamic_linking") {
        return true;
    }
    if seen.iter().any(|done| done == feature) {
        return false;
    }
    seen.push(feature.to_string());
    project::strings(&features[feature])
        .iter()
        .any(|enabled| links_dynamically(features, enabled, seen))
}

/// What the editor crate is generated from.
struct EditorCrate<'a> {
    /// The project's package name.
    project: &'a str,
    /// Where the project gets the engine.
    engine: &'a EngineSource,
    /// The project's library, when it has one.
    library: Option<&'a Library>,
    /// The library's function the editor calls with its `App`.
    hook: Option<&'a str>,
    /// The project's `[profile]` tables.
    profiles: &'a str,
}

impl EditorCrate<'_> {
    /// The crate's `Cargo.toml`.
    fn cargo_toml(&self) -> String {
        let mut text = format!(
            "# The editor for {project}, generated by `hearth editor`, which rewrites it.\n\n[package]\nname = \"{EDITOR}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\nhearth = {engine}\n",
            project = self.project,
            engine = self.engine.toml(&[EDITOR_FEATURE]),
        );
        if let Some(library) = self.library {
            text.push_str(&format!("{} = {}\n", library.crate_name, library.toml()));
        }
        if !self.profiles.is_empty() {
            text.push_str(
                "\n# The project's profiles, so that the engine builds the same way and the build is shared.\n",
            );
            text.push_str(self.profiles);
        }
        text.push_str("\n# A workspace of its own, apart from the project's.\n[workspace]\n");
        text
    }

    /// The crate's `src/main.rs`.
    fn main_rs(&self) -> String {
        let mut text = format!(
            "//! The editor for {}, generated by `hearth editor`, which rewrites it.\n\nuse hearth::prelude::*;\n",
            self.project
        );
        let mut call = String::new();
        if let Some(library) = self.library {
            match self.hook {
                Some(hook) => call = format!("    {}::{hook}(&mut app);\n", library.crate_name),
                None => text.push_str(&format!(
                    "// Links the game, so that the editor knows the types it reflects.\nuse {} as _;\n",
                    library.crate_name
                )),
            }
        }
        text.push_str(
            "\nfn main() {\n    let mut app = App::new();\n    app.add_plugins((\n        DefaultPlugins.set(WindowPlugin {\n            primary_window: Some(hearth::editor::editor_window()),\n            ..default()\n        }),\n        hearth::editor::EditorPlugin::default(),\n    ));\n",
        );
        text.push_str(&call);
        text.push_str("    app.run();\n}\n");
        text
    }

    /// The crate's files, by path relative to its directory.
    fn files(&self) -> [(&'static str, String); 2] {
        [
            ("Cargo.toml", self.cargo_toml()),
            ("src/main.rs", self.main_rs()),
        ]
    }
}

/// Writes `bytes` to `path` unless the file already holds them, so cargo sees no change and
/// doesn't rebuild. Returns whether it wrote.
fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<bool, CliError> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| CliError::io(parent.display(), error))?;
    }
    std::fs::write(path, bytes).map_err(|error| CliError::io(path.display(), error))?;
    Ok(true)
}

/// Copies the project's `Cargo.lock` into the editor crate in `dir` when it changed since the
/// last copy, so the editor builds the versions and git revisions the project does. Cargo adds
/// the editor's own entries to the copy, so the last copy is kept beside it, as `project.lock`,
/// to compare with. Returns whether it copied.
fn sync_lock(project_lock: &Path, dir: &Path) -> Result<bool, CliError> {
    let Ok(lock) = std::fs::read(project_lock) else {
        return Ok(false);
    };
    let copied = write_if_changed(&dir.join("project.lock"), &lock)?;
    let lock_path = dir.join("Cargo.lock");
    if copied || !lock_path.is_file() {
        std::fs::write(&lock_path, &lock)
            .map_err(|error| CliError::io(lock_path.display(), error))?;
        return Ok(true);
    }
    Ok(false)
}

/// The flags Play passes to `hearth run`, so the game builds the way the editor did: with the same
/// profile, features, toolchain and cargo arguments.
fn play_args(args: &EditorArgs, toolchain: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    if args.release {
        out.push("--release".to_string());
    }
    if let Some(profile) = &args.profile {
        out.extend(["--profile".to_string(), profile.clone()]);
    }
    if !args.features.is_empty() {
        out.extend(["--features".to_string(), args.features.join(",")]);
    }
    if let Some(manifest) = &args.manifest_path {
        let manifest = std::path::absolute(manifest).unwrap_or_else(|_| manifest.clone());
        out.extend([
            "--manifest-path".to_string(),
            manifest.display().to_string(),
        ]);
    }
    if let Some(toolchain) = toolchain {
        out.extend(["--toolchain".to_string(), toolchain.to_string()]);
    }
    out.extend(
        args.cargo_args
            .iter()
            .map(|arg| format!("--cargo-arg={arg}")),
    );
    out
}

/// The environment the editor runs with, besides what `hearth run` gives an app.
fn editor_env(
    project: &Path,
    package: &str,
    port: u16,
    scene: Option<&Path>,
    cli: Option<&Path>,
    play_args: &[String],
) -> Vec<(&'static str, OsString)> {
    let mut env = vec![
        ("HEARTH_PROJECT", project.as_os_str().to_owned()),
        // The asset server's base, which `assets/` is under.
        ("HEARTH_ASSET_ROOT", project.as_os_str().to_owned()),
        ("HEARTH_EDITOR_PACKAGE", OsString::from(package)),
        ("HEARTH_REMOTE_PORT", OsString::from(port.to_string())),
    ];
    if let Some(cli) = cli {
        env.push(("HEARTH_CLI", cli.as_os_str().to_owned()));
    }
    if let Some(scene) = scene {
        let scene = std::path::absolute(scene).unwrap_or_else(|_| scene.to_path_buf());
        env.push(("HEARTH_EDITOR_SCENE", scene.into_os_string()));
    }
    if !play_args.is_empty() {
        // One per line: no flag or value has a line break in it.
        env.push((
            "HEARTH_EDITOR_RUN_ARGS",
            OsString::from(play_args.join(
                "
",
            )),
        ));
    }
    env
}

/// `hearth editor`.
pub fn editor(ctx: &mut Ctx, args: EditorArgs) -> Result<Outcome, CliError> {
    // The MCP server talks over stdout, which an attached editor would write to.
    let detach = args.detach || ctx.mcp.is_some();
    let toolchain = project::toolchain(args.toolchain.as_deref());
    let manifest = args.manifest_path.as_deref();
    let meta = project::metadata(manifest, toolchain.as_deref(), true)?;
    let package = select_package(&meta, args.package.as_deref(), manifest)?;

    let engine = engine_source(package, &meta.all)?;
    if let EngineSource::Registry {
        index: Some(index), ..
    } = &engine
    {
        ctx.out.warn(format!(
            "The project gets the engine from the registry at {index}, but the editor gets it from crates.io"
        ));
    }
    let hook = editor_hook(&package.metadata)?;
    let library = library_crate(package)
        .filter(|_| package.name != "hearth")
        .map(|crate_name| Library {
            crate_name,
            package: package.name.clone(),
            dir: package.dir().to_path_buf(),
            features: args
                .features
                .iter()
                .map(|feature| feature.trim().to_string())
                .filter(|feature| !feature.is_empty())
                .collect(),
            defaults: static_defaults(&package.features),
        });
    match (&library, &hook) {
        (None, Some(hook)) if package.name != "hearth" => {
            return Err(CliError::new(
                "INVALID_EDITOR_HOOK",
                Exit::Usage,
                format!(
                    "`{}` names `{hook}` as its editor hook, but has no library to call it in",
                    package.name
                ),
            )
            .hint("Move the game's code to `src/lib.rs`, as `hearth new` projects have it."));
        }
        (None, _) if package.name != "hearth" => ctx.out.warn(format!(
            "`{}` has no library, so the editor won't know the game's types. Move the game's code to `src/lib.rs`, as `hearth new` projects have it.",
            package.name
        )),
        _ => {}
    }
    let root_manifest =
        std::fs::read_to_string(meta.workspace_root.join("Cargo.toml")).unwrap_or_default();
    if has_patches(&root_manifest) {
        ctx.out.warn(
            "The editor doesn't apply the project's `[patch]` and `[replace]` tables, so it may build other versions of the patched crates.",
        );
    }

    let dir = meta.target_directory.join(EDITOR);
    let generated = EditorCrate {
        project: &package.name,
        engine: &engine,
        library: library.as_ref(),
        hook: hook.as_deref().filter(|_| library.is_some()),
        profiles: &profile_tables(&root_manifest),
    };
    let mut written = Vec::new();
    for (path, text) in generated.files() {
        if write_if_changed(&dir.join(path), text.as_bytes())? {
            written.push(path);
        }
    }
    if sync_lock(&meta.workspace_root.join("Cargo.lock"), &dir)? {
        written.push("Cargo.lock");
    }

    let label = format!("editor {}", package.name);
    let (name, replaced) = prepare_session(
        ctx,
        detach,
        args.name.as_deref(),
        SESSION,
        &label,
        package.dir(),
    )?;
    let port = args.port.unwrap_or_else(free_port);

    let mut flags = vec![
        "--manifest-path".to_string(),
        display_path(&dir.join("Cargo.toml")),
        "--bin".to_string(),
        EDITOR.to_string(),
    ];
    if args.release {
        flags.push("--release".to_string());
    }
    if let Some(profile) = &args.profile {
        flags.extend(["--profile".to_string(), profile.clone()]);
    }
    flags.extend(args.cargo_args.iter().cloned());
    ctx.out.info(format!(
        "Building the editor for {} in {}",
        package.name,
        display_path(&dir)
    ));
    let started = Instant::now();
    // From the project's directory, cargo reads the project's `.cargo/config.toml` and rustup
    // its toolchain file, as for `hearth run`.
    let build = project::build_with(&ctx.out, "build", &flags, toolchain.as_deref(), |cargo| {
        cargo
            .current_dir(package.dir())
            .env("CARGO_TARGET_DIR", &meta.target_directory);
    })?;
    if !build.success {
        return Err(build.failure());
    }
    let executable = built_executable(&build, "bin", EDITOR)?;

    let cli = std::env::current_exe().ok();
    let mut command = Command::new(&executable);
    command
        .envs(run_env(package.dir(), &executable, toolchain.as_deref()))
        .envs(editor_env(
            package.dir(),
            &package.name,
            port,
            args.scene.as_deref(),
            cli.as_deref(),
            &play_args(&args, toolchain.as_deref()),
        ));
    let launch = Launch {
        command,
        executable,
        label,
        port,
        project: package.dir().to_path_buf(),
        package: package.name.clone(),
        args: Vec::new(),
        remote: build.libraries.contains("hearth_remote"),
        engine: build.libraries.contains("hearth_app"),
        started,
    };
    let mut outcome = if detach {
        launch_detached(
            ctx,
            launch,
            &Detached {
                name,
                replaced,
                no_wait: args.no_wait,
                wait_timeout: args.wait_timeout,
            },
        )?
    } else {
        launch_attached(ctx, launch)?
    };
    outcome.data["editor"] = json!({
        "crate": display_path(&dir),
        "engine": engine.toml(&[EDITOR_FEATURE]),
        "library": library.as_ref().map(|library| library.crate_name.clone()),
        "hook": generated.hook,
        "written": written,
    });
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Dependency, Target};

    fn package(name: &str, dependencies: Vec<Dependency>) -> Package {
        Package {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            source: None,
            manifest_path: PathBuf::from("/games").join(name).join("Cargo.toml"),
            targets: vec![Target {
                name: name.to_string(),
                kinds: vec!["lib".to_string()],
                required_features: Vec::new(),
                src_path: PathBuf::from("src/lib.rs"),
            }],
            dependencies,
            default_run: None,
            rust_version: None,
            metadata: Value::Null,
            features: Value::Null,
        }
    }

    fn hearth(source: Option<&str>, req: &str, path: Option<&str>) -> Dependency {
        Dependency {
            name: "hearth".to_string(),
            source: source.map(str::to_string),
            req: req.to_string(),
            kind: None,
            path: path.map(PathBuf::from),
            registry: None,
        }
    }

    fn git(url: &str, reference: Option<(&str, &str)>) -> EngineSource {
        EngineSource::Git {
            url: url.to_string(),
            reference: reference.map(|(kind, value)| (kind.to_string(), value.to_string())),
        }
    }

    const HEARTH: &str = "https://github.com/noahsabaj/hearth";

    #[test]
    fn derives_the_engine_source() {
        let path = package("game", vec![hearth(None, "*", Some("/dev/hearth"))]);
        assert_eq!(
            engine_source(&path, &[]).unwrap(),
            EngineSource::Path(PathBuf::from("/dev/hearth"))
        );
        let branch = package(
            "game",
            vec![hearth(
                Some(&format!("git+{HEARTH}?branch=main")),
                "*",
                None,
            )],
        );
        assert_eq!(
            engine_source(&branch, &[]).unwrap(),
            git(HEARTH, Some(("branch", "main")))
        );
        let tag = package(
            "game",
            vec![hearth(
                Some(&format!("git+{HEARTH}?tag=v0.20%2Bhearth")),
                "*",
                None,
            )],
        );
        assert_eq!(
            engine_source(&tag, &[]).unwrap(),
            git(HEARTH, Some(("tag", "v0.20+hearth")))
        );
        let head = package(
            "game",
            vec![hearth(Some(&format!("git+{HEARTH}")), "*", None)],
        );
        assert_eq!(engine_source(&head, &[]).unwrap(), git(HEARTH, None));
        let registry = package(
            "game",
            vec![hearth(
                Some("registry+https://github.com/rust-lang/crates.io-index"),
                "^0.19",
                None,
            )],
        );
        assert_eq!(
            engine_source(&registry, &[]).unwrap(),
            EngineSource::Registry {
                version: "^0.19".to_string(),
                index: None
            }
        );

        // A dev dependency on another copy doesn't win over the normal one.
        let mut dev = hearth(
            Some("registry+https://github.com/rust-lang/crates.io-index"),
            "^0.18",
            None,
        );
        dev.kind = Some("dev".to_string());
        let both = package("game", vec![dev, hearth(None, "*", Some("/dev/hearth"))]);
        assert_eq!(
            engine_source(&both, &[]).unwrap(),
            EngineSource::Path(PathBuf::from("/dev/hearth"))
        );

        // Through another crate, the resolved package says where the engine is from.
        let indirect = package("game", Vec::new());
        let mut resolved = package("hearth", Vec::new());
        resolved.source = Some(format!("git+{HEARTH}?rev=1a2b3c#1a2b3c4d5e"));
        assert_eq!(
            engine_source(&indirect, &[resolved.clone()]).unwrap(),
            git(HEARTH, Some(("rev", "1a2b3c")))
        );
        resolved.source = Some("registry+https://github.com/rust-lang/crates.io-index".into());
        resolved.version = "0.19.1".to_string();
        assert_eq!(
            engine_source(&indirect, &[resolved]).unwrap(),
            EngineSource::Registry {
                version: "=0.19.1".to_string(),
                index: None
            }
        );

        // The engine's own repository edits itself.
        let engine = package("hearth", Vec::new());
        assert_eq!(
            engine_source(&engine, &[]).unwrap(),
            EngineSource::Path(PathBuf::from("/games/hearth"))
        );
        assert_eq!(
            engine_source(&indirect, &[]).unwrap_err().code,
            "NO_ENGINE_DEPENDENCY"
        );
    }

    #[test]
    fn writes_engine_dependencies() {
        let features = [EDITOR_FEATURE];
        assert_eq!(
            EngineSource::Path(PathBuf::from(r"C:\dev\hearth")).toml(&features),
            r#"{ path = "C:/dev/hearth", features = ["hearth_editor"] }"#
        );
        assert_eq!(
            git(HEARTH, Some(("branch", "main"))).toml(&features),
            r#"{ git = "https://github.com/noahsabaj/hearth", branch = "main", features = ["hearth_editor"] }"#
        );
        assert_eq!(
            git(HEARTH, None).toml(&[]),
            r#"{ git = "https://github.com/noahsabaj/hearth" }"#
        );
        assert_eq!(
            EngineSource::Registry {
                version: "^0.19".to_string(),
                index: None
            }
            .toml(&features),
            r#"{ version = "^0.19", features = ["hearth_editor"] }"#
        );
    }

    #[test]
    fn reads_editor_hooks() {
        let hook = |metadata: Value| editor_hook(&metadata);
        assert_eq!(hook(Value::Null).unwrap(), None);
        assert_eq!(hook(json!({ "hearth": {} })).unwrap(), None);
        assert_eq!(
            hook(json!({ "hearth": { "editor-hook": "register_types" } })).unwrap(),
            Some("register_types".to_string())
        );
        assert_eq!(
            hook(json!({ "hearth": { "editor-hook": "crate::editor::setup" } })).unwrap(),
            Some("editor::setup".to_string())
        );
        for invalid in [
            json!("register types"),
            json!("1st"),
            json!(""),
            json!("editor::"),
            json!("super::setup"),
            json!(42),
        ] {
            let error = hook(json!({ "hearth": { "editor-hook": invalid } })).unwrap_err();
            assert_eq!(error.code, "INVALID_EDITOR_HOOK", "{invalid}");
        }
    }

    #[test]
    fn copies_profiles_and_notices_patches() {
        let manifest = "[package]\nname = \"game\"\n\n[profile.dev]\nopt-level = 1 # fast enough\n\n# The engine.\n[profile.dev.package.\"*\"]\nopt-level = 3\ndebug = \"line-tables-only\"\n\n[[bin]]\nname = \"game\"\n\n[ profile.release ]\nlto = true\n";
        assert_eq!(
            profile_tables(manifest),
            "[profile.dev]\nopt-level = 1 # fast enough\n\n[profile.dev.package.\"*\"]\nopt-level = 3\ndebug = \"line-tables-only\"\n\n[profile.release]\nlto = true\n"
        );
        assert_eq!(profile_tables("[package]\nname = \"game\"\n"), "");
        assert!(!has_patches(manifest));
        assert!(has_patches(
            "[patch.crates-io]\nhearth = { path = \"../hearth\" }\n"
        ));
        assert!(has_patches(
            "[patch.\"https://github.com/noahsabaj/hearth\"]\n"
        ));
    }

    fn library() -> Library {
        Library {
            crate_name: "my_game".to_string(),
            package: "my-game".to_string(),
            dir: PathBuf::from("C:/games/my-game"),
            features: Vec::new(),
            defaults: None,
        }
    }

    #[test]
    fn the_editor_drops_the_games_dynamic_linking() {
        let features = serde_json::json!({
            "default": ["remote", "fast", "dynamic_linking"],
            "remote": ["hearth/hearth_remote"],
            "fast": ["dynamic_linking"],
            "dynamic_linking": ["hearth/dynamic_linking"],
        });
        assert_eq!(static_defaults(&features), Some(vec!["remote".to_string()]));
        let plain =
            serde_json::json!({ "default": ["remote"], "remote": ["hearth/hearth_remote"] });
        assert_eq!(static_defaults(&plain), None);
        assert_eq!(static_defaults(&Value::Null), None);

        let mut library = library();
        library.defaults = Some(vec!["remote".to_string()]);
        library.features = vec!["debug".to_string()];
        assert_eq!(
            library.toml(),
            r#"{ path = "C:/games/my-game", package = "my-game", default-features = false, features = ["remote", "debug"] }"#
        );
    }

    #[test]
    fn generates_the_editor_crate() {
        let engine = EngineSource::Path(PathBuf::from("C:/dev/hearth"));
        let library = library();
        let generated = EditorCrate {
            project: "my-game",
            engine: &engine,
            library: Some(&library),
            hook: Some("register_types"),
            profiles: "[profile.dev]\nopt-level = 1\n",
        };
        assert_eq!(
            generated.cargo_toml(),
            r#"# The editor for my-game, generated by `hearth editor`, which rewrites it.

[package]
name = "hearth-editor"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
hearth = { path = "C:/dev/hearth", features = ["hearth_editor"] }
my_game = { path = "C:/games/my-game", package = "my-game" }

# The project's profiles, so that the engine builds the same way and the build is shared.
[profile.dev]
opt-level = 1

# A workspace of its own, apart from the project's.
[workspace]
"#
        );
        assert_eq!(
            generated.main_rs(),
            r#"//! The editor for my-game, generated by `hearth editor`, which rewrites it.

use hearth::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(hearth::editor::editor_window()),
            ..default()
        }),
        hearth::editor::EditorPlugin::default(),
    ));
    my_game::register_types(&mut app);
    app.run();
}
"#
        );

        // Without a hook the library is still linked; without one, nothing of the game is.
        let mut featured = library.clone();
        featured.features = vec!["debug".to_string()];
        let unhooked = EditorCrate {
            library: Some(&featured),
            hook: None,
            profiles: "",
            ..generated
        };
        let main = unhooked.main_rs();
        assert!(main.contains("\nuse my_game as _;\n"), "{main}");
        assert!(!main.contains("register_types"));
        let toml = unhooked.cargo_toml();
        assert!(toml.contains(
            "my_game = { path = \"C:/games/my-game\", package = \"my-game\", features = [\"debug\"] }\n"
        ));
        assert!(!toml.contains("[profile"));
        assert!(toml.ends_with("\n[workspace]\n"));
        let alone = EditorCrate {
            library: None,
            hook: None,
            ..generated
        };
        assert!(!alone.main_rs().contains("my_game"));
        assert!(!alone.cargo_toml().contains("my_game"));
    }

    #[test]
    fn rewrites_only_what_changed() {
        let dir =
            std::env::temp_dir().join(format!("hearth-cli-editor-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("src/main.rs");
        assert!(write_if_changed(&file, b"fn main() {}\n").unwrap());
        assert!(!write_if_changed(&file, b"fn main() {}\n").unwrap());
        assert!(write_if_changed(&file, b"fn main() { run() }\n").unwrap());
        assert_eq!(std::fs::read(&file).unwrap(), b"fn main() { run() }\n");

        let project_lock = dir.join("project/Cargo.lock");
        let editor = dir.join("editor");
        assert!(!sync_lock(&project_lock, &editor).unwrap(), "no lock yet");
        write_if_changed(&project_lock, b"version = 4\n").unwrap();
        assert!(sync_lock(&project_lock, &editor).unwrap());
        assert_eq!(
            std::fs::read(editor.join("Cargo.lock")).unwrap(),
            b"version = 4\n"
        );
        // Cargo adds the editor's own packages; that copy stays until the project's changes.
        std::fs::write(editor.join("Cargo.lock"), b"version = 4\n# hearth-editor\n").unwrap();
        assert!(!sync_lock(&project_lock, &editor).unwrap());
        assert_eq!(
            std::fs::read(editor.join("Cargo.lock")).unwrap(),
            b"version = 4\n# hearth-editor\n"
        );
        std::fs::write(&project_lock, b"version = 4\n# updated\n").unwrap();
        assert!(sync_lock(&project_lock, &editor).unwrap());
        assert_eq!(
            std::fs::read(editor.join("Cargo.lock")).unwrap(),
            b"version = 4\n# updated\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gives_the_editor_its_environment() {
        let project = Path::new("/games/my_game");
        let env = editor_env(
            project,
            "my_game",
            15704,
            Some(Path::new("assets/scenes/main.scn.ron")),
            Some(Path::new("/bin/hearth")),
            &["--release".to_string(), "--cargo-arg=--locked".to_string()],
        );
        let get = |name: &str| {
            env.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| PathBuf::from(value))
        };
        assert_eq!(get("HEARTH_PROJECT").unwrap(), project);
        assert_eq!(get("HEARTH_ASSET_ROOT").unwrap(), project);
        assert_eq!(get("HEARTH_EDITOR_PACKAGE").unwrap(), Path::new("my_game"));
        assert_eq!(get("HEARTH_REMOTE_PORT").unwrap(), Path::new("15704"));
        assert_eq!(get("HEARTH_CLI").unwrap(), Path::new("/bin/hearth"));
        let scene = get("HEARTH_EDITOR_SCENE").unwrap();
        assert!(scene.is_absolute() && scene.ends_with("assets/scenes/main.scn.ron"));
        assert_eq!(
            get("HEARTH_EDITOR_RUN_ARGS").unwrap(),
            Path::new(
                "--release
--cargo-arg=--locked"
            )
        );
        let bare = editor_env(project, "my_game", 15702, None, None, &[]);
        assert!(!bare.iter().any(|(key, _)| {
            matches!(
                *key,
                "HEARTH_EDITOR_SCENE" | "HEARTH_CLI" | "HEARTH_EDITOR_RUN_ARGS"
            )
        }));
    }

    #[test]
    fn finds_the_library() {
        let mut game = package("my-game", Vec::new());
        assert_eq!(library_crate(&game).as_deref(), Some("my_game"));
        game.targets[0].kinds = vec!["bin".to_string()];
        assert_eq!(library_crate(&game), None);
    }
}
