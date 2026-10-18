//! Cargo: reading the project, building it with structured diagnostics, and running what it built
//! the way `cargo run` would.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use serde_json::{json, Value};

use crate::cli::CargoArgs;
use crate::output::{strip_ansi, CliError, Exit, Format, Output};
use crate::registry::edit_distance;

/// The engine repository `hearth new` depends on unless told otherwise.
pub const HEARTH_GIT: &str = "https://github.com/noahsabaj/hearth";

/// How many warnings a build reports in full. Errors are always reported.
const MAX_WARNINGS: usize = 100;

/// The toolchain to build with: the flag, else `HEARTH_TOOLCHAIN`.
pub fn toolchain(flag: Option<&str>) -> Option<String> {
    flag.map(str::to_string)
        .or_else(|| std::env::var("HEARTH_TOOLCHAIN").ok())
        .map(|name| name.trim().trim_start_matches('+').to_string())
        .filter(|name| !name.is_empty())
}

/// A command for a rustup-managed tool, such as `cargo`, using `toolchain` when set.
pub fn tool(program: &str, toolchain: Option<&str>) -> Command {
    let mut command = Command::new(program);
    if let Some(toolchain) = toolchain {
        command.arg(format!("+{toolchain}"));
    }
    command.stdin(Stdio::null());
    command
}

/// The error for a tool that couldn't be started.
pub fn spawn_error(program: &str, toolchain: Option<&str>, error: &std::io::Error) -> CliError {
    if error.kind() == std::io::ErrorKind::NotFound {
        return CliError::new(
            "CARGO_NOT_FOUND",
            Exit::Error,
            format!("`{program}` isn't installed, or isn't on PATH"),
        )
        .hint("Install Rust with rustup: https://rustup.rs");
    }
    let toolchain = toolchain
        .map(|name| format!(" +{name}"))
        .unwrap_or_default();
    CliError::new(
        "CARGO_FAILED",
        Exit::Error,
        format!("Couldn't run `{program}{toolchain}`: {error}"),
    )
}

/// The error for cargo exiting with a failure before it built anything.
fn cargo_error(what: &str, toolchain: Option<&str>, stderr: &str) -> CliError {
    let stderr = strip_ansi(stderr);
    if stderr.contains("could not find `Cargo.toml`") {
        return CliError::new("NO_PROJECT", Exit::Error, "There's no Cargo project here")
            .hint("Run this in a project directory, pass --manifest-path, or create a project with `hearth new <name>`.");
    }
    if let Some(toolchain) = toolchain
        && (stderr.contains("is not installed") || stderr.contains("not a directory"))
    {
        return CliError::new(
            "TOOLCHAIN_NOT_INSTALLED",
            Exit::Error,
            format!("The `{toolchain}` toolchain isn't installed"),
        )
        .hint(format!(
            "Install it with `rustup toolchain install {toolchain}`."
        ));
    }
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tail = lines[lines.len().saturating_sub(20)..].join("\n");
    CliError::new(
        "CARGO_FAILED",
        Exit::Error,
        format!("`cargo {what}` failed:\n{tail}"),
    )
}

/// A build target of a package: a binary, an example, the library and so on.
#[derive(Clone, Debug)]
pub struct Target {
    /// Its name.
    pub name: String,
    /// Its kinds, such as `bin`, `example` or `lib`.
    pub kinds: Vec<String>,
    /// Features it needs.
    pub required_features: Vec<String>,
    /// Its main source file.
    pub src_path: PathBuf,
}

impl Target {
    /// Whether the target is of `kind`.
    pub fn is(&self, kind: &str) -> bool {
        self.kinds.iter().any(|candidate| candidate == kind)
    }
}

/// A dependency as a package's `Cargo.toml` declares it.
#[derive(Clone, Debug, Default)]
pub struct Dependency {
    /// The package it names, which is not the name it's renamed to.
    pub name: String,
    /// Where it comes from, such as `git+https://github.com/noahsabaj/hearth?branch=main` or
    /// `registry+https://github.com/rust-lang/crates.io-index`. `None` for a path dependency.
    pub source: Option<String>,
    /// Its version requirement, such as `^0.19`.
    pub req: String,
    /// `dev` or `build`, or `None` for a normal dependency.
    pub kind: Option<String>,
    /// The directory of a path dependency.
    pub path: Option<PathBuf>,
    /// The index of the registry it comes from, when that isn't crates.io.
    pub registry: Option<String>,
}

/// A package of the workspace.
#[derive(Clone, Debug)]
pub struct Package {
    /// Its name.
    pub name: String,
    /// Its version.
    pub version: String,
    /// Where it comes from, as `cargo metadata` resolved it; `None` for a local package.
    pub source: Option<String>,
    /// Its `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Its targets.
    pub targets: Vec<Target>,
    /// Its dependencies, as its `Cargo.toml` declares them.
    pub dependencies: Vec<Dependency>,
    /// The binary `cargo run` runs by default.
    pub default_run: Option<String>,
    /// The Rust version it needs.
    pub rust_version: Option<String>,
    /// Its `[package.metadata]`.
    pub metadata: Value,
}

impl Package {
    /// The directory of its `Cargo.toml`.
    pub fn dir(&self) -> &Path {
        self.manifest_path.parent().unwrap_or(Path::new("."))
    }

    /// Its targets of `kind`.
    pub fn targets_of(&self, kind: &str) -> Vec<&Target> {
        self.targets
            .iter()
            .filter(|target| target.is(kind))
            .collect()
    }
}

/// What `cargo metadata` says about the workspace.
#[derive(Clone, Debug)]
pub struct Metadata {
    /// The workspace's packages.
    pub members: Vec<Package>,
    /// Every package, dependencies included, when they were resolved.
    pub all: Vec<Package>,
    /// The resolved dependency graph, when it was resolved.
    pub resolve: Value,
    /// The directory of the workspace's root `Cargo.toml`, where its `Cargo.lock` is.
    pub workspace_root: PathBuf,
    /// Where the workspace builds to.
    pub target_directory: PathBuf,
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_package(package: &Value) -> Package {
    let text = |key: &str| package[key].as_str().map(str::to_string);
    Package {
        name: text("name").unwrap_or_default(),
        version: text("version").unwrap_or_default(),
        source: text("source"),
        manifest_path: PathBuf::from(text("manifest_path").unwrap_or_default()),
        targets: package["targets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|target| Target {
                name: target["name"].as_str().unwrap_or_default().to_string(),
                kinds: strings(&target["kind"]),
                required_features: strings(&target["required-features"]),
                src_path: PathBuf::from(target["src_path"].as_str().unwrap_or_default()),
            })
            .collect(),
        dependencies: package["dependencies"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|dependency| {
                let text = |key: &str| dependency[key].as_str().map(str::to_string);
                Dependency {
                    name: text("name").unwrap_or_default(),
                    source: text("source"),
                    req: text("req").unwrap_or_default(),
                    kind: text("kind"),
                    path: text("path").map(PathBuf::from),
                    registry: text("registry"),
                }
            })
            .collect(),
        default_run: text("default_run"),
        rust_version: text("rust_version"),
        metadata: package["metadata"].clone(),
    }
}

/// Reads the workspace with `cargo metadata`. Without `deps`, dependencies aren't resolved,
/// which is fast and works offline.
pub fn metadata(
    manifest: Option<&Path>,
    toolchain: Option<&str>,
    deps: bool,
) -> Result<Metadata, CliError> {
    let mut command = tool("cargo", toolchain);
    command.args(["metadata", "--format-version", "1"]);
    if !deps {
        command.arg("--no-deps");
    }
    if let Some(manifest) = manifest {
        command.arg("--manifest-path").arg(manifest);
    }
    let output = command
        .output()
        .map_err(|error| spawn_error("cargo", toolchain, &error))?;
    if !output.status.success() {
        return Err(cargo_error(
            "metadata",
            toolchain,
            &String::from_utf8_lossy(&output.stderr),
        ));
    }
    let raw: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        CliError::internal(format!("`cargo metadata` printed invalid JSON: {error}"))
    })?;
    let members: HashSet<&str> = raw["workspace_members"]
        .as_array()
        .map(|ids| ids.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let packages = raw["packages"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    Ok(Metadata {
        members: packages
            .iter()
            .filter(|package| {
                package["id"]
                    .as_str()
                    .is_some_and(|id| members.contains(id))
            })
            .map(parse_package)
            .collect(),
        all: if deps {
            packages.iter().map(parse_package).collect()
        } else {
            Vec::new()
        },
        resolve: raw["resolve"].clone(),
        workspace_root: PathBuf::from(raw["workspace_root"].as_str().unwrap_or_default()),
        target_directory: PathBuf::from(raw["target_directory"].as_str().unwrap_or_default()),
    })
}

/// The `Cargo.toml` nearest the current directory, or `manifest` when given.
fn nearest_manifest(manifest: Option<&Path>) -> Option<PathBuf> {
    if let Some(manifest) = manifest {
        return std::fs::canonicalize(manifest).ok();
    }
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .map(|dir| dir.join("Cargo.toml"))
        .find(|path| path.is_file())
        .and_then(|path| std::fs::canonicalize(path).ok())
}

/// The package a command works on: the one named with `-p`, else the one whose `Cargo.toml` is
/// nearest the current directory, else the workspace's only package with binaries.
pub fn select_package<'a>(
    meta: &'a Metadata,
    spec: Option<&str>,
    manifest: Option<&Path>,
) -> Result<&'a Package, CliError> {
    let names = || {
        meta.members
            .iter()
            .map(|package| package.name.clone())
            .collect::<Vec<_>>()
    };
    if let Some(spec) = spec {
        let name = spec.split('@').next().unwrap_or(spec);
        return meta
            .members
            .iter()
            .find(|package| package.name == name)
            .ok_or_else(|| {
                CliError::new(
                    "UNKNOWN_PACKAGE",
                    Exit::Usage,
                    format!("The workspace has no package named `{name}`"),
                )
                .data(json!({ "candidates": names() }))
            });
    }
    if let Some(nearest) = nearest_manifest(manifest)
        && let Some(package) = meta.members.iter().find(|package| {
            std::fs::canonicalize(&package.manifest_path).is_ok_and(|path| path == nearest)
        })
    {
        return Ok(package);
    }
    let with_bins: Vec<&Package> = meta
        .members
        .iter()
        .filter(|package| !package.targets_of("bin").is_empty())
        .collect();
    match (meta.members.as_slice(), with_bins.as_slice()) {
        ([only], _) => Ok(only),
        (_, [only]) => Ok(*only),
        _ => Err(CliError::new(
            "AMBIGUOUS_PACKAGE",
            Exit::Usage,
            "The workspace has several packages",
        )
        .hint("Pick one with -p <package>.")
        .data(json!({ "candidates": names() }))),
    }
}

/// What `hearth run` runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunTarget {
    /// A binary.
    Bin(String),
    /// An example.
    Example(String),
}

impl RunTarget {
    /// `bin` or `example`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Bin(_) => "bin",
            Self::Example(_) => "example",
        }
    }

    /// The target's name.
    pub fn name(&self) -> &str {
        match self {
            Self::Bin(name) | Self::Example(name) => name,
        }
    }

    /// The cargo arguments that select it.
    pub fn cargo_args(&self) -> [String; 2] {
        [format!("--{}", self.kind()), self.name().to_string()]
    }
}

/// The names of `targets` close to `name`, for suggestions.
fn similar(targets: &[&Target], name: &str) -> Vec<String> {
    let lower = name.to_lowercase();
    let mut close: Vec<String> = targets
        .iter()
        .filter(|target| {
            let candidate = target.name.to_lowercase();
            candidate.contains(&lower)
                || lower.contains(&candidate)
                || edit_distance(&candidate, &lower) <= 3
        })
        .map(|target| target.name.clone())
        .collect();
    close.sort();
    close.truncate(12);
    close
}

/// Chooses what to run: `--bin`, `--example`, else the package's binary.
pub fn run_target<'a>(
    package: &'a Package,
    bin: Option<&str>,
    example: Option<&str>,
) -> Result<(RunTarget, &'a Target), CliError> {
    let pick = |kind: &str, name: &str| {
        let targets = package.targets_of(kind);
        match targets.iter().find(|target| target.name == name) {
            Some(target) => Ok(*target),
            None => Err(CliError::new(
                if kind == "example" {
                    "UNKNOWN_EXAMPLE"
                } else {
                    "UNKNOWN_BIN"
                },
                Exit::Usage,
                format!("`{}` has no {kind} named `{name}`", package.name),
            )
            .hint(if kind == "example" {
                "`hearth examples` lists the examples."
            } else {
                "Pass one of the package's binaries."
            })
            .data(json!({ "candidates": similar(&targets, name) }))),
        }
    };
    if let Some(name) = example {
        return Ok((RunTarget::Example(name.to_string()), pick("example", name)?));
    }
    if let Some(name) = bin.or(package.default_run.as_deref()) {
        return Ok((RunTarget::Bin(name.to_string()), pick("bin", name)?));
    }
    let bins = package.targets_of("bin");
    match bins.as_slice() {
        [only] => Ok((RunTarget::Bin(only.name.clone()), only)),
        [] => Err(CliError::new(
            "NO_BINARY",
            Exit::Usage,
            format!("`{}` has no binary to run", package.name),
        )
        .hint("Run an example with --example <name>; `hearth examples` lists them.")),
        many => Err(CliError::new(
            "AMBIGUOUS_TARGET",
            Exit::Usage,
            format!("`{}` has several binaries", package.name),
        )
        .hint("Pick one with --bin <name>, or set `default-run` in Cargo.toml.")
        .data(json!({ "candidates": many.iter().map(|target| target.name.clone()).collect::<Vec<_>>() }))),
    }
}

/// The cargo flags for the profile, features and extra arguments of `args`, with
/// `extra_features` enabled too.
pub fn cargo_flags(args: &CargoArgs, extra_features: &[String]) -> Vec<String> {
    let mut flags = Vec::new();
    if let Some(manifest) = &args.manifest_path {
        flags.push("--manifest-path".to_string());
        flags.push(manifest.display().to_string());
    }
    if args.release {
        flags.push("--release".to_string());
    }
    if let Some(profile) = &args.profile {
        flags.push("--profile".to_string());
        flags.push(profile.clone());
    }
    let features: Vec<&str> = args
        .features
        .iter()
        .chain(extra_features)
        .map(|feature| feature.trim())
        .filter(|feature| !feature.is_empty())
        .collect();
    if !features.is_empty() {
        flags.push("--features".to_string());
        flags.push(features.join(","));
    }
    if args.all_features {
        flags.push("--all-features".to_string());
    }
    if args.no_default_features {
        flags.push("--no-default-features".to_string());
    }
    flags.extend(args.cargo_args.iter().cloned());
    flags
}

/// An executable a build produced.
#[derive(Clone, Debug)]
pub struct Artifact {
    /// The package it belongs to.
    pub package: String,
    /// The target's name.
    pub target: String,
    /// The target's kinds.
    pub kinds: Vec<String>,
    /// The executable.
    pub executable: PathBuf,
}

/// What a build did.
#[derive(Debug, Default)]
pub struct Build {
    /// Whether it succeeded.
    pub success: bool,
    /// How long it took.
    pub seconds: f64,
    /// The errors and warnings, deduplicated, at most [`MAX_WARNINGS`] warnings.
    pub diagnostics: Vec<Value>,
    /// How many errors there were.
    pub errors: usize,
    /// How many warnings there were.
    pub warnings: usize,
    /// The executables it produced.
    pub artifacts: Vec<Artifact>,
    /// The names of every library compiled into the build, such as `bevy_remote`.
    pub libraries: HashSet<String>,
    /// The end of cargo's own output, when it was captured.
    pub cargo_output: Vec<String>,
}

impl Build {
    /// The error for a failed build.
    pub fn failure(&self) -> CliError {
        let errors: Vec<&Value> = self
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic["level"] == "error")
            .collect();
        let message = if self.errors > 0 {
            let first = errors
                .first()
                .map(|error| {
                    let location = match (error["file"].as_str(), error["line"].as_u64()) {
                        (Some(file), Some(line)) => format!(" at {file}:{line}"),
                        _ => String::new(),
                    };
                    format!(
                        ": {}{location}",
                        error["message"].as_str().unwrap_or_default()
                    )
                })
                .unwrap_or_default();
            format!(
                "The build failed with {} error{}{first}",
                self.errors,
                if self.errors == 1 { "" } else { "s" }
            )
        } else {
            let last = self
                .cargo_output
                .iter()
                .rev()
                .find(|line| line.starts_with("error"))
                .cloned();
            match last {
                Some(line) => format!("The build failed: {line}"),
                None => "The build failed".to_string(),
            }
        };
        let output = self.cargo_output.join("\n");
        let hint = if output.contains("requires rustc") || output.contains("rust-version") {
            Some("The project needs a newer Rust: run `rustup update`, or pick a newer toolchain with --toolchain (with --cargo-arg=--ignore-rust-version if it's a nightly of that version).")
        } else if self.errors > 0 {
            Some("Each error's file, line and message is in `errors`; `rendered` has the compiler's full explanation.")
        } else {
            None
        };
        let error = CliError::new("BUILD_FAILED", Exit::BuildFailed, message).data(json!({
            "errors": errors,
            "error_count": self.errors,
            "warning_count": self.warnings,
            "cargo_output": self.cargo_output,
        }));
        match hint {
            Some(hint) => error.hint(hint),
            None => error,
        }
    }
}

/// The name in a cargo package id, such as `my_game` in `path+file:///games/my_game#0.1.0` or
/// `serde` in `registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0`.
pub fn package_name(id: &str) -> String {
    if let Some((url, fragment)) = id.rsplit_once('#') {
        return match fragment.split_once('@') {
            Some((name, _)) => name.to_string(),
            None => url
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(url)
                .to_string(),
        };
    }
    // The older format: `name version (source)`.
    id.split(' ').next().unwrap_or(id).to_string()
}

/// A diagnostic from a `compiler-message`, unless it's only a summary such as "aborting due to
/// 2 previous errors".
fn diagnostic(message: &Value) -> Option<Value> {
    let inner = &message["message"];
    let level = inner["level"].as_str()?;
    let level = if level.starts_with("error") {
        "error"
    } else if level == "warning" {
        "warning"
    } else {
        return None;
    };
    let text = inner["message"].as_str().unwrap_or_default();
    let spans = inner["spans"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if spans.is_empty()
        && (text.starts_with("aborting due to")
            || text.ends_with("warning emitted")
            || text.ends_with("warnings emitted"))
    {
        return None;
    }
    let primary = spans
        .iter()
        .find(|span| span["is_primary"] == true)
        .or_else(|| spans.first());
    let field = |key: &str| primary.map(|span| span[key].clone()).unwrap_or(Value::Null);
    Some(json!({
        "level": level,
        "message": text,
        "code": inner["code"]["code"],
        "file": field("file_name"),
        "line": field("line_start"),
        "column": field("column_start"),
        "end_line": field("line_end"),
        "end_column": field("column_end"),
        "label": field("label"),
        "package": message["package_id"].as_str().map(package_name),
        "target": message["target"]["name"],
        "rendered": inner["rendered"].as_str().map(strip_ansi),
    }))
}

/// Runs `cargo <command>` (`build` or `check`) with `flags`, reading its JSON messages.
///
/// For people, cargo's progress and the compiler's diagnostics are shown as they happen; for
/// machines, they are only collected.
pub fn build(
    out: &Output,
    command: &str,
    flags: &[String],
    toolchain: Option<&str>,
) -> Result<Build, CliError> {
    build_with(out, command, flags, toolchain, |_| {})
}

/// [`build`], with `configure` setting up the cargo command further, such as its working
/// directory and environment.
pub fn build_with(
    out: &Output,
    command: &str,
    flags: &[String],
    toolchain: Option<&str>,
    configure: impl FnOnce(&mut Command),
) -> Result<Build, CliError> {
    let show = out.format == Format::Human && !out.capture;
    let format = if show && out.color {
        "json-diagnostic-rendered-ansi"
    } else {
        "json"
    };
    let mut cargo = tool("cargo", toolchain);
    cargo
        .arg(command)
        .arg(format!("--message-format={format}"))
        .args(out.quiet.then_some("--quiet"))
        .args(flags)
        .stdout(Stdio::piped())
        .stderr(if show {
            Stdio::inherit()
        } else {
            Stdio::piped()
        });
    configure(&mut cargo);
    let started = Instant::now();
    let mut child = cargo
        .spawn()
        .map_err(|error| spawn_error("cargo", toolchain, &error))?;
    let stderr = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });
    let mut build = Build::default();
    let mut seen = HashSet::new();
    let mut finished = None;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                if show {
                    let _ = writeln!(std::io::stderr(), "{line}");
                }
                continue;
            };
            match message["reason"].as_str() {
                Some("compiler-message") => {
                    let Some(diagnostic) = diagnostic(&message) else {
                        continue;
                    };
                    let rendered = message["message"]["rendered"].as_str().unwrap_or_default();
                    if !seen.insert(strip_ansi(rendered)) {
                        continue;
                    }
                    if show {
                        let mut stderr = std::io::stderr().lock();
                        let _ = write!(stderr, "{rendered}");
                        let _ = stderr.flush();
                    }
                    if diagnostic["level"] == "error" {
                        build.errors += 1;
                        build.diagnostics.push(diagnostic);
                    } else {
                        build.warnings += 1;
                        if build.warnings <= MAX_WARNINGS {
                            build.diagnostics.push(diagnostic);
                        }
                    }
                }
                Some("compiler-artifact") => {
                    let target = &message["target"];
                    let name = target["name"].as_str().unwrap_or_default().to_string();
                    let kinds = strings(&target["kind"]);
                    if kinds.iter().any(|kind| kind.contains("lib")) {
                        build.libraries.insert(name.clone());
                    }
                    if let Some(executable) = message["executable"].as_str() {
                        build.artifacts.push(Artifact {
                            package: message["package_id"]
                                .as_str()
                                .map(package_name)
                                .unwrap_or_default(),
                            target: name,
                            kinds,
                            executable: PathBuf::from(executable),
                        });
                    }
                }
                Some("build-finished") => finished = message["success"].as_bool(),
                _ => {}
            }
        }
    }
    let status = child
        .wait()
        .map_err(|error| CliError::io("waiting for cargo", error))?;
    let stderr = stderr
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    let lines: Vec<String> = strip_ansi(&stderr)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect();
    build.cargo_output = lines[lines.len().saturating_sub(40)..].to_vec();
    build.success = finished.unwrap_or(false) && status.success();
    build.seconds = (started.elapsed().as_secs_f64() * 10.0).round() / 10.0;
    Ok(build)
}

/// The library directory of the toolchain's standard library, which dynamically linked
/// builds load from.
fn target_libdir(toolchain: Option<&str>) -> Option<PathBuf> {
    let output = tool("rustc", toolchain)
        .args(["--print", "target-libdir"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !dir.is_empty()).then(|| PathBuf::from(dir))
}

/// The environment `cargo run` gives an executable: `CARGO_MANIFEST_DIR`, which Bevy finds its
/// `assets` folder with, and the library path that dynamically linked builds need.
pub fn run_env(
    package_dir: &Path,
    executable: &Path,
    toolchain: Option<&str>,
) -> Vec<(OsString, OsString)> {
    let mut env = vec![(
        OsString::from("CARGO_MANIFEST_DIR"),
        package_dir.as_os_str().to_owned(),
    )];
    let mut dirs = Vec::new();
    if let Some(dir) = executable.parent() {
        let profile = if dir.file_name().is_some_and(|name| name == "examples") {
            dir.parent().unwrap_or(dir)
        } else {
            dir
        };
        dirs.push(profile.join("deps"));
        dirs.push(profile.to_path_buf());
    }
    dirs.extend(target_libdir(toolchain));
    let var = if cfg!(windows) {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_FALLBACK_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    if let Some(existing) = std::env::var_os(var) {
        dirs.extend(std::env::split_paths(&existing));
    }
    if let Ok(joined) = std::env::join_paths(dirs) {
        env.push((OsString::from(var), joined));
    }
    env
}

/// Parses a version such as `1.97.1` or `1.97.0-nightly` into numbers to compare.
pub fn version_numbers(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text
        .trim()
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_package_ids() {
        assert_eq!(
            package_name("path+file:///C:/games/my_game#0.1.0"),
            "my_game"
        );
        assert_eq!(
            package_name("path+file:///C:/bevy/crates/bevy_cli#bevy_cli@0.20.0-dev"),
            "bevy_cli"
        );
        assert_eq!(
            package_name("registry+https://github.com/rust-lang/crates.io-index#serde@1.0.228"),
            "serde"
        );
        assert_eq!(
            package_name("serde 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)"),
            "serde"
        );
    }

    #[test]
    fn reads_diagnostics() {
        let message = json!({
            "reason": "compiler-message",
            "package_id": "path+file:///games/my_game#0.1.0",
            "target": { "name": "my_game", "kind": ["bin"] },
            "message": {
                "level": "error",
                "message": "mismatched types",
                "code": { "code": "E0308" },
                "rendered": "\u{1b}[31merror\u{1b}[0m: mismatched types\n",
                "spans": [
                    { "is_primary": false, "file_name": "src/other.rs", "line_start": 1 },
                    { "is_primary": true, "file_name": "src/main.rs", "line_start": 12, "column_start": 5, "line_end": 12, "column_end": 9, "label": "expected `u32`" }
                ]
            }
        });
        let diagnostic = diagnostic(&message).unwrap();
        assert_eq!(diagnostic["level"], "error");
        assert_eq!(diagnostic["code"], "E0308");
        assert_eq!(diagnostic["file"], "src/main.rs");
        assert_eq!(diagnostic["line"], 12);
        assert_eq!(diagnostic["package"], "my_game");
        assert_eq!(diagnostic["rendered"], "error: mismatched types\n");
        let summary = json!({ "message": { "level": "error", "message": "aborting due to 1 previous error", "spans": [] } });
        assert!(super::diagnostic(&summary).is_none());
        let note = json!({ "message": { "level": "note", "message": "x", "spans": [] } });
        assert!(super::diagnostic(&note).is_none());
    }

    #[test]
    fn compares_versions() {
        assert_eq!(version_numbers("1.97.1"), Some((1, 97, 1)));
        assert_eq!(version_numbers("1.97.0-nightly"), Some((1, 97, 0)));
        assert_eq!(version_numbers("1.85"), Some((1, 85, 0)));
        assert!(version_numbers("1.97.0-nightly") < version_numbers("1.97.1"));
        assert_eq!(version_numbers("nightly"), None);
    }

    #[test]
    fn chooses_run_targets() {
        let target = |name: &str, kind: &str| Target {
            name: name.to_string(),
            kinds: vec![kind.to_string()],
            required_features: Vec::new(),
            src_path: PathBuf::new(),
        };
        let mut package = Package {
            name: "game".to_string(),
            version: "0.1.0".to_string(),
            source: None,
            manifest_path: PathBuf::from("Cargo.toml"),
            targets: vec![
                target("game", "bin"),
                target("server", "example"),
                target("game", "lib"),
            ],
            dependencies: Vec::new(),
            default_run: None,
            rust_version: None,
            metadata: Value::Null,
        };
        assert_eq!(
            run_target(&package, None, None).unwrap().0,
            RunTarget::Bin("game".into())
        );
        assert_eq!(
            run_target(&package, None, Some("server")).unwrap().0,
            RunTarget::Example("server".into())
        );
        let unknown = run_target(&package, None, Some("sever")).unwrap_err();
        assert_eq!(unknown.code, "UNKNOWN_EXAMPLE");
        assert_eq!(unknown.data.unwrap()["candidates"], json!(["server"]));
        package.targets.push(target("tool", "bin"));
        assert_eq!(
            run_target(&package, None, None).unwrap_err().code,
            "AMBIGUOUS_TARGET"
        );
        package.default_run = Some("tool".into());
        assert_eq!(
            run_target(&package, None, None).unwrap().0,
            RunTarget::Bin("tool".into())
        );
    }
}
