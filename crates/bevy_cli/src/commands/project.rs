//! Project commands: creating, building and running projects, and managing the sessions that
//! `hearth run --detach` starts.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::screenshot::display_path;
use crate::brp::{Client, DEFAULT_PORT};
use crate::cli::{
    BuildArgs, CargoArgs, DoctorArgs, ExamplesArgs, Level, LogsArgs, NewArgs, PsArgs, RunArgs,
    SkillClient, StopArgs, Template,
};
use crate::context::Ctx;
use crate::http::{self, Endpoint};
use crate::known;
use crate::output::{strip_ansi, table, CliError, Exit, Outcome, Style};
use crate::project::{self, cargo_flags, run_env, run_target, select_package, version_numbers};
use crate::session::{self, Session};
use crate::target::{self, Source};

/// The port Bevy's render sub-app listens on. It can't be changed, so `hearth run` never picks it.
const RENDER_PORT: u16 = 15703;

// --- hearth new ---

/// Names `hearth new` refuses because they clash with Bevy or Rust.
const RESERVED: &[&str] = &[
    "bevy",
    "test",
    "core",
    "std",
    "alloc",
    "proc_macro",
    "self",
    "super",
    "crate",
    "build",
    "deps",
];

fn validate_name(name: &str) -> Result<(), CliError> {
    let valid = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        return Err(CliError::usage(format!(
            "`{name}` can't be a package name: use letters, digits, `_` and `-`, starting with a letter"
        )));
    }
    if RESERVED.contains(&name.to_lowercase().replace('-', "_").as_str()) {
        return Err(CliError::usage(format!(
            "`{name}` is reserved; pick another name"
        )));
    }
    Ok(())
}

/// `my_game` becomes `My Game`.
fn title_of(name: &str) -> String {
    name.split(['_', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A TOML basic string.
fn toml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The `bevy` dependency of a new project.
fn bevy_dependency(args: &NewArgs) -> Result<String, CliError> {
    if let Some(path) = &args.engine_path {
        let manifest = std::fs::read_to_string(path.join("Cargo.toml")).unwrap_or_default();
        if !manifest.contains("name = \"bevy\"") {
            return Err(CliError::usage(format!(
                "`{}` isn't an engine checkout: it has no Cargo.toml for the `bevy` package",
                path.display()
            )));
        }
        let path =
            std::fs::canonicalize(path).map_err(|error| CliError::io(path.display(), error))?;
        let path = display_path(&path).replace('\\', "/");
        return Ok(format!("{{ path = {} }}", toml_string(&path)));
    }
    if let Some(version) = &args.bevy_version {
        return Ok(toml_string(version));
    }
    let git = args.engine_git.as_deref().unwrap_or(project::HEARTH_GIT);
    Ok(match &args.engine_branch {
        Some(branch) => format!(
            "{{ git = {}, branch = {} }}",
            toml_string(git),
            toml_string(branch)
        ),
        None => format!("{{ git = {} }}", toml_string(git)),
    })
}

/// Initializes a git repository, unless `dir` is already in one. Returns whether it did.
fn git_init(dir: &Path) -> Result<bool, String> {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    };
    match git(&["rev-parse", "--is-inside-work-tree"]) {
        Ok(output)
            if output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == "true" =>
        {
            return Ok(false);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err("git isn't installed, so no repository was created".to_string());
        }
        _ => {}
    }
    match git(&["init", "--quiet"]) {
        Ok(output) if output.status.success() => Ok(true),
        Ok(output) => Err(format!(
            "`git init` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => Err(format!("`git init` failed: {error}")),
    }
}

/// The `.cargo/config.toml` that `hearth new` writes on Windows when rustup's `rust-lld` is
/// available. It only applies to Windows targets, so the project still builds elsewhere.
const WINDOWS_LINKER: &str = "\
# Link with LLVM's linker, which ships with Rust, instead of MSVC's link.exe: it links a game
# about a third faster, and linking is most of the time an incremental rebuild takes.
[target.x86_64-pc-windows-msvc]
linker = \"rust-lld.exe\"

[target.aarch64-pc-windows-msvc]
linker = \"rust-lld.exe\"
";

/// Whether an executable called `name` is on `PATH`.
fn on_path(name: &str) -> bool {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(&file).is_file()))
}

/// `hearth new`.
pub fn new(ctx: &mut Ctx, args: NewArgs) -> Result<Outcome, CliError> {
    validate_name(&args.name)?;
    let dir = args.path.join(&args.name);
    if dir.exists() {
        let empty = std::fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_none());
        if !dir.is_dir() || (!empty && !args.force) {
            return Err(CliError::new(
                "DIRECTORY_NOT_EMPTY",
                Exit::Usage,
                format!(
                    "`{}` already exists and isn't empty",
                    display_path(&std::path::absolute(&dir).unwrap_or_else(|_| dir.clone()))
                ),
            )
            .hint("Pick another name or --path, or pass --force to write into it anyway."));
        }
    }
    let bevy = bevy_dependency(&args)?;
    let title = title_of(&args.name);
    let (main, template) = match args.template {
        Template::ThreeD => (include_str!("../../templates/3d.rs"), "3d"),
        Template::TwoD => (include_str!("../../templates/2d.rs"), "2d"),
        Template::Minimal => (include_str!("../../templates/minimal.rs"), "minimal"),
    };
    let fill = |text: &str| {
        text.replace("{{name}}", &args.name)
            .replace("{{title}}", &title)
            .replace("{{bevy}}", &bevy)
    };
    let mut files = vec![
        (
            "Cargo.toml",
            fill(include_str!("../../templates/Cargo.toml.template")),
        ),
        ("src/main.rs", fill(main)),
        (".gitignore", "/target\n".to_string()),
    ];
    if cfg!(windows) && on_path("rust-lld") {
        files.push((".cargo/config.toml", WINDOWS_LINKER.to_string()));
    }
    for (path, text) in &files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| CliError::io(parent.display(), error))?;
        }
        std::fs::write(&path, text).map_err(|error| CliError::io(path.display(), error))?;
    }
    let git = if args.no_git {
        false
    } else {
        match git_init(&dir) {
            Ok(created) => created,
            Err(warning) => {
                ctx.out.warn(warning);
                false
            }
        }
    };
    let shown = display_path(&std::fs::canonicalize(&dir).unwrap_or(dir.clone()));
    let cd = if args.path == Path::new(".") {
        args.name.clone()
    } else {
        shown.clone()
    };
    let next = vec![
        format!("cd {cd}"),
        "hearth run --detach".to_string(),
        "hearth tree".to_string(),
    ];
    let human = format!(
        "Created {} ({template} template) in {shown}\n\nNext:\n{}",
        ctx.out.paint(&args.name, Style::Bold),
        next.iter()
            .map(|step| format!("  {step}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(Outcome::new(
        json!({
            "name": args.name,
            "path": shown,
            "template": template,
            "bevy": bevy,
            "files": files.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
            "git": git,
            "next": next,
        }),
        human,
    ))
}

// --- hearth build, hearth check ---

/// The features `target` requires that `args` doesn't enable.
fn missing_features(args: &CargoArgs, target: &project::Target) -> Vec<String> {
    if args.all_features {
        return Vec::new();
    }
    target
        .required_features
        .iter()
        .filter(|feature| {
            !args
                .features
                .iter()
                .any(|enabled| enabled.trim() == feature.as_str())
        })
        .cloned()
        .collect()
}

fn executables(build: &project::Build) -> Vec<Value> {
    build
        .artifacts
        .iter()
        .filter(|artifact| {
            artifact
                .kinds
                .iter()
                .any(|kind| kind == "bin" || kind == "example")
        })
        .map(|artifact| {
            json!({
                "name": artifact.target,
                "kind": artifact.kinds.first(),
                "package": artifact.package,
                "executable": display_path(&artifact.executable),
            })
        })
        .collect()
}

/// `hearth build` and `hearth check`.
pub fn build(ctx: &mut Ctx, args: BuildArgs, check: bool) -> Result<Outcome, CliError> {
    let cargo = &args.cargo;
    let toolchain = project::toolchain(cargo.toolchain.as_deref());
    let mut selection = Vec::new();
    let mut extra = Vec::new();
    if let Some(package) = &cargo.package {
        selection.extend(["-p".to_string(), package.clone()]);
    }
    if cargo.bin.is_some() || cargo.example.is_some() {
        let meta = project::metadata(cargo.manifest_path.as_deref(), toolchain.as_deref(), false)?;
        let package = select_package(
            &meta,
            cargo.package.as_deref(),
            cargo.manifest_path.as_deref(),
        )?;
        let (chosen, target) = run_target(package, cargo.bin.as_deref(), cargo.example.as_deref())?;
        extra = missing_features(cargo, target);
        if !extra.is_empty() {
            ctx.out.info(format!(
                "Enabling the features `{}` requires: {}",
                chosen.name(),
                extra.join(", ")
            ));
        }
        if cargo.package.is_none() {
            selection.extend(["-p".to_string(), package.name.clone()]);
        }
        selection.extend(chosen.cargo_args());
    }
    selection.extend(cargo_flags(cargo, &extra));
    let command = if check { "check" } else { "build" };
    let build = project::build(&ctx.out, command, &selection, toolchain.as_deref())?;
    if !build.success {
        return Err(build.failure());
    }
    let executables = executables(&build);
    let warnings = match build.warnings {
        0 => String::new(),
        1 => ", 1 warning".to_string(),
        n => format!(", {n} warnings"),
    };
    let mut human = vec![format!(
        "{} in {:.1}s{warnings}",
        if check { "Checked" } else { "Built" },
        build.seconds
    )];
    for executable in &executables {
        human.push(format!(
            "  {} {}",
            executable["name"].as_str().unwrap_or_default(),
            ctx.out.paint(
                executable["executable"].as_str().unwrap_or_default(),
                Style::Dim
            )
        ));
    }
    Ok(Outcome::new(
        json!({
            "success": true,
            "seconds": build.seconds,
            "warning_count": build.warnings,
            "warnings": build.diagnostics,
            "executables": executables,
        }),
        human.join("\n"),
    ))
}

// --- hearth run ---

/// Whether two paths name the same directory.
fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The first port from 15702 that nothing listens on and no session uses.
fn free_port() -> u16 {
    let taken: Vec<u16> = session::list()
        .into_iter()
        .filter(Session::is_running)
        .map(|session| session.port)
        .collect();
    (DEFAULT_PORT..DEFAULT_PORT.saturating_add(100))
        .filter(|&port| port != RENDER_PORT)
        .find(|&port| {
            // Binding is instant, where probing a free port waits out a timeout on Windows.
            !taken.contains(&port) && std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
        })
        .unwrap_or(DEFAULT_PORT)
}

/// Starts `command` so that it outlives this process and doesn't get the terminal's Ctrl+C.
#[cfg(windows)]
fn spawn_detached(command: &mut Command) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // Leave the job this process may run in, so the app isn't ended along with it.
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB);
    match command.spawn() {
        // Access denied: the job doesn't allow leaving it.
        Err(error) if error.raw_os_error() == Some(5) => {
            command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
            command.spawn()
        }
        result => result,
    }
}

/// Starts `command` so that it outlives this process and doesn't get the terminal's Ctrl+C.
#[cfg(unix)]
fn spawn_detached(command: &mut Command) -> std::io::Result<Child> {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    command.spawn()
}

/// Starts `command` so that it outlives this process.
#[cfg(not(any(windows, unix)))]
fn spawn_detached(command: &mut Command) -> std::io::Result<Child> {
    command.spawn()
}

/// The last `count` lines of a log.
fn log_tail(path: &Path, count: usize) -> Vec<String> {
    let text = std::fs::read(path).unwrap_or_default();
    let text = strip_ansi(&String::from_utf8_lossy(&text));
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(count)..]
        .iter()
        .map(|line| line.trim_end().to_string())
        .collect()
}

/// `hearth run`.
pub fn run(ctx: &mut Ctx, args: RunArgs) -> Result<Outcome, CliError> {
    // The MCP server talks over stdout, which an attached app would write to.
    let detach = args.detach || ctx.mcp.is_some();
    let cargo = &args.cargo;
    let toolchain = project::toolchain(cargo.toolchain.as_deref());
    let meta = project::metadata(cargo.manifest_path.as_deref(), toolchain.as_deref(), false)?;
    let package = select_package(
        &meta,
        cargo.package.as_deref(),
        cargo.manifest_path.as_deref(),
    )?;
    let (chosen, target) = run_target(package, cargo.bin.as_deref(), cargo.example.as_deref())?;
    let missing = missing_features(cargo, target);
    if !missing.is_empty() {
        ctx.out.info(format!(
            "Enabling the features `{}` requires: {}",
            chosen.name(),
            missing.join(", ")
        ));
    }
    let mut flags = vec!["-p".to_string(), package.name.clone()];
    flags.extend(chosen.cargo_args());
    flags.extend(cargo_flags(cargo, &missing));
    let label = format!("{} {}", chosen.kind(), chosen.name());

    // Session names are unique: starting a session with the name of a running one from this
    // project replaces it. That's also what rebuilding needs on Windows, where the executable
    // of a running app can't be replaced.
    let mut replaced = false;
    let name = if detach {
        let base = session::sanitize(args.name.as_deref().unwrap_or(chosen.name()));
        match session::find(&base).filter(Session::is_running) {
            Some(running) if args.name.is_some() || same_dir(&running.project, package.dir()) => {
                ctx.out
                    .info(format!("Stopping the running session {base} first"));
                stop_session(&running, false)?;
                running.forget();
                replaced = true;
                base
            }
            Some(_) => session::unique_name(&base),
            None => base,
        }
    } else {
        if cfg!(windows)
            && let Some(running) = session::list().into_iter().find(|session| {
                session.target == label
                    && same_dir(&session.project, package.dir())
                    && session.is_running()
            })
        {
            ctx.out.warn(format!(
                "Session {0} runs this {label}, so the build may fail to replace its executable. `hearth stop {0}` stops it.",
                running.name
            ));
        }
        String::new()
    };
    let port = args.port.unwrap_or_else(free_port);

    let started = Instant::now();
    let build = project::build(&ctx.out, "build", &flags, toolchain.as_deref())?;
    if !build.success {
        return Err(build.failure());
    }
    let artifact = build
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.target == chosen.name()
                && artifact.kinds.iter().any(|kind| kind == chosen.kind())
        })
        .ok_or_else(|| {
            CliError::internal(format!(
                "cargo didn't report an executable for {} `{}`",
                chosen.kind(),
                chosen.name()
            ))
        })?;
    let executable = artifact.executable.clone();
    let remote = build.libraries.contains("bevy_remote");

    let mut command = Command::new(&executable);
    command
        .args(&args.args)
        .envs(run_env(package.dir(), &executable, toolchain.as_deref()))
        .env("HEARTH_REMOTE_PORT", port.to_string());

    if !detach {
        if remote {
            ctx.out
                .info(format!("Running {label}, answering BRP on port {port}"));
        } else if build.libraries.contains("bevy_app") {
            ctx.out.info(
                "This build doesn't include `bevy_remote`, so live commands won't reach the app.",
            );
        }
        let status = command.status().map_err(|error| {
            CliError::io(format!("could not start {}", executable.display()), error)
        })?;
        if status.success() {
            return Ok(Outcome::new(json!({ "exit_code": 0 }), String::new()));
        }
        let code = status.code();
        return Err(CliError::new(
            "APP_FAILED",
            Exit::Error,
            match code {
                Some(code) => format!("The app exited with code {code}"),
                None => "The app was ended by a signal".to_string(),
            },
        )
        .data(json!({ "exit_code": code })));
    }

    let logs = session::logs_dir();
    std::fs::create_dir_all(&logs).map_err(|error| CliError::io(logs.display(), error))?;
    let log = logs.join(format!("{name}.log"));
    let file = File::create(&log).map_err(|error| CliError::io(log.display(), error))?;
    let file_err = file
        .try_clone()
        .map_err(|error| CliError::io(log.display(), error))?;
    command
        .stdin(Stdio::null())
        .stdout(file)
        .stderr(file_err)
        .env("NO_COLOR", "1");
    let mut child = spawn_detached(&mut command).map_err(|error| {
        CliError::io(format!("could not start {}", executable.display()), error)
    })?;
    let session = Session {
        name: name.clone(),
        pid: child.id(),
        port,
        project: package.dir().to_path_buf(),
        package: package.name.clone(),
        target: label.clone(),
        executable: executable.clone(),
        args: args.args.clone(),
        log: log.clone(),
        started_at: session::now_millis(),
    };
    session.save()?;

    let endpoint = Endpoint::localhost(port);
    let mut data = json!({
        "session": name,
        "pid": session.pid,
        "port": port,
        "endpoint": endpoint.to_string(),
        "target": label,
        "executable": display_path(&executable),
        "log": display_path(&log),
        "replaced": replaced,
        "ready": Value::Null,
    });
    let next = format!("  hearth status --app {name}\n  hearth logs {name}\n  hearth stop {name}");
    if !remote {
        ctx.out.warn(format!(
            "This build doesn't include `bevy_remote`, so live commands won't reach {name}. Enable the feature (`hearth new` projects have it by default) and add `RemotePlugin` and `RemoteHttpPlugin`."
        ));
        data["ready"] = json!(false);
        return Ok(Outcome::new(
            data,
            format!(
                "Started {label} as session {name} (pid {})\n{next}",
                session.pid
            ),
        ));
    }
    if args.no_wait {
        return Ok(Outcome::new(
            data,
            format!(
                "Started {label} as session {name} (pid {}); it will answer on {endpoint}\n{next}",
                session.pid
            ),
        ));
    }

    ctx.out.info(format!(
        "Started {label} as session {name} (pid {}); waiting for it to answer on {endpoint}",
        session.pid
    ));
    let client = Client::new(endpoint.clone(), ctx.timeout);
    let wait = Duration::from_secs_f64(args.wait_timeout.max(0.0));
    let ready = client.wait_until_ready(wait, || matches!(child.try_wait(), Ok(None)));
    match ready {
        Ok(()) => {}
        Err(error) if error.code == "APP_EXITED" => {
            let code = child
                .try_wait()
                .ok()
                .flatten()
                .and_then(|status| status.code());
            let tail = log_tail(&log, 30);
            return Err(CliError::new(
                "APP_EXITED",
                Exit::Error,
                format!(
                    "{name} exited{} before it answered over BRP",
                    code.map(|code| format!(" with code {code}"))
                        .unwrap_or_default()
                ),
            )
            .hint(format!(
                "The end of its log is in `log_tail`; `hearth logs {name}` shows all of it."
            ))
            .data(json!({
                "session": name,
                "exit_code": code,
                "log": display_path(&log),
                "log_tail": tail,
            })));
        }
        Err(error) if error.code == "TIMEOUT" => {
            return Err(error
                .hint(format!(
                    "{name} is still running, but nothing answers on port {port}. The app needs `RemotePlugin` and `RemoteHttpPlugin` listening on the port in `HEARTH_REMOTE_PORT`, as `hearth new` projects do; or pass the port it listens on with --port. `hearth logs {name}` shows its output and `hearth stop {name}` ends it."
                ))
                .data(json!({ "session": name, "pid": session.pid, "port": port, "log": display_path(&log) })));
        }
        Err(error) => return Err(error),
    }
    let info = client.call("app.info", None).ok();
    data["ready"] = json!(true);
    data["app"] = info.clone().unwrap_or(Value::Null);
    data["seconds"] = json!((started.elapsed().as_secs_f64() * 10.0).round() / 10.0);
    let version = info
        .as_ref()
        .and_then(|info| info["bevy_version"].as_str())
        .map(|version| format!(", engine {version}"))
        .unwrap_or_default();
    Ok(Outcome::new(
        data,
        format!(
            "Session {} is ready: {label} (pid {}{version}) answers on {}\n{next}",
            ctx.out.paint(&name, Style::Bold),
            session.pid,
            ctx.out.paint(&endpoint.to_string(), Style::Id)
        ),
    ))
}

// --- sessions ---

fn unknown_session(name: &str) -> CliError {
    let names: Vec<String> = session::list()
        .into_iter()
        .map(|session| session.name)
        .collect();
    CliError::new(
        "UNKNOWN_SESSION",
        Exit::Unreachable,
        format!("There's no session named `{name}`"),
    )
    .hint("`hearth ps` lists the sessions.")
    .data(json!({ "candidates": names }))
}

/// The session a command is about: the one named, else the one `--app` or the project chose,
/// else the only one.
fn pick_session(ctx: &mut Ctx, name: Option<&str>) -> Result<Session, CliError> {
    if let Some(name) = name {
        return session::find(name).ok_or_else(|| unknown_session(name));
    }
    let target = ctx.target()?;
    if let Some(session) = &target.session {
        return Ok(session.clone());
    }
    if matches!(target.source, Source::Flag | Source::Env) {
        return Err(CliError::new(
            "NOT_A_SESSION",
            Exit::Usage,
            format!(
                "The app at {} wasn't started with `hearth run --detach`",
                target.endpoint
            ),
        )
        .hint("`hearth exit` asks any app to exit."));
    }
    let sessions = session::list();
    let running: Vec<&Session> = sessions
        .iter()
        .filter(|session| session.is_running())
        .collect();
    match (running.as_slice(), sessions.as_slice()) {
        ([only], _) => Ok((*only).clone()),
        ([], [only]) => Ok(only.clone()),
        ([], []) => Err(
            CliError::new("NO_SESSION", Exit::Unreachable, "There are no sessions")
                .hint("Start one with `hearth run --detach`."),
        ),
        _ => Err(CliError::new(
            "AMBIGUOUS_SESSION",
            Exit::Unreachable,
            "Several sessions exist; say which one",
        )
        .hint("Pass the session name, or --app <name>.")
        .data(json!({
            "candidates": sessions.iter().map(|session| session.name.clone()).collect::<Vec<_>>(),
        }))),
    }
}

/// Seconds as `45s`, `12m`, `3h 12m` or `2d 3h`.
fn duration_text(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h {}m", secs / 3600, secs % 3600 / 60),
        _ => format!("{}d {}h", secs / 86400, secs % 86400 / 3600),
    }
}

/// `hearth ps`.
pub fn ps(ctx: &mut Ctx, args: PsArgs) -> Result<Outcome, CliError> {
    let sessions = session::list();
    let found = if args.no_scan {
        Vec::new()
    } else {
        target::scan(&target::scan_ports())
    };
    let mut rows = Vec::new();
    let mut items = Vec::new();
    let mut ports = Vec::new();
    for session in &sessions {
        let running = session.is_running();
        let answering =
            running && http::is_listening(&Endpoint::localhost(session.port), http::LOOPBACK_PROBE);
        if running {
            ports.push(session.port);
        }
        let status = match (running, answering) {
            (false, _) => "exited",
            (true, true) => "running",
            (true, false) => "running, not answering",
        };
        rows.push(vec![
            ctx.out.paint(&session.name, Style::Bold),
            session.pid.to_string(),
            session.port.to_string(),
            status.to_string(),
            if running {
                duration_text(session.uptime_secs())
            } else {
                "-".to_string()
            },
            session.target.clone(),
        ]);
        items.push(json!({
            "name": session.name,
            "pid": session.pid,
            "port": session.port,
            "status": if running { "running" } else { "exited" },
            "answering": answering,
            "target": session.target,
            "package": session.package,
            "project": display_path(&session.project),
            "log": display_path(&session.log),
            "started_at": session.started_at,
            "uptime_secs": running.then(|| session.uptime_secs()),
        }));
    }
    let others: Vec<&target::Found> = found
        .iter()
        .filter(|found| !ports.contains(&found.endpoint.port))
        .collect();
    let mut human = if rows.is_empty() {
        "No sessions. Start one with `hearth run --detach`.".to_string()
    } else {
        table(
            ctx.out.color,
            &["NAME", "PID", "PORT", "STATUS", "UPTIME", "TARGET"],
            &rows,
        )
    };
    if !others.is_empty() {
        human.truncate(human.trim_end().len());
        human.push_str("\n\nOther apps answering BRP:\n");
        for found in &others {
            human.push_str(&format!("  {}\n", found.describe()));
        }
    }
    Ok(Outcome::new(
        json!({
            "sessions": items,
            "apps": found
                .iter()
                .map(|found| json!({
                    "endpoint": found.endpoint.to_string(),
                    "port": found.endpoint.port,
                    "session": sessions
                        .iter()
                        .find(|session| session.port == found.endpoint.port && ports.contains(&session.port))
                        .map(|session| session.name.clone()),
                    "app": found.info,
                }))
                .collect::<Vec<_>>(),
        }),
        human,
    ))
}

/// The level of a log line, if it says: Bevy writes lines as
/// `2025-01-01T12:00:00.000000Z  INFO target: message`, and panics as `thread '…' panicked`.
fn line_level(line: &str) -> Option<Level> {
    for word in line.split_whitespace().take(3) {
        match word {
            "TRACE" => return Some(Level::Trace),
            "DEBUG" => return Some(Level::Debug),
            "INFO" => return Some(Level::Info),
            "WARN" => return Some(Level::Warn),
            "ERROR" => return Some(Level::Error),
            _ => {}
        }
    }
    (line.starts_with("thread '") && line.contains("panicked")).then_some(Level::Error)
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Trace => "trace",
        Level::Debug => "debug",
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    }
}

/// Reads a log, filtering lines like `hearth logs` does.
struct LogReader {
    path: PathBuf,
    cursor: u64,
    /// The level of the last line, which lines without one continue.
    level: Level,
    min_level: Option<Level>,
    grep: Option<String>,
    color: bool,
}

/// A line of a log.
struct LogLine {
    level: Level,
    text: String,
}

impl LogReader {
    /// Reads the lines after the cursor. A partial last line is left for the next read, unless
    /// `all` is set because nothing will be written anymore.
    fn read(
        &mut self,
        all: bool,
        out: &mut crate::output::Output,
    ) -> Result<Vec<LogLine>, CliError> {
        let mut file =
            File::open(&self.path).map_err(|error| CliError::io(self.path.display(), error))?;
        let len = file
            .metadata()
            .map_err(|error| CliError::io(self.path.display(), error))?
            .len();
        if self.cursor > len {
            out.warn("The log is shorter than the cursor: the session restarted, so reading from the start");
            self.cursor = 0;
        }
        file.seek(SeekFrom::Start(self.cursor))
            .map_err(|error| CliError::io(self.path.display(), error))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| CliError::io(self.path.display(), error))?;
        let complete = if all {
            bytes.len()
        } else {
            bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1)
        };
        self.cursor += complete as u64;
        let text = String::from_utf8_lossy(&bytes[..complete]);
        let mut lines = Vec::new();
        for raw in text.lines() {
            let raw = raw.trim_end_matches('\r');
            let plain = strip_ansi(raw);
            if let Some(level) = line_level(&plain) {
                self.level = level;
            }
            if self.min_level.is_some_and(|min| self.level < min) {
                continue;
            }
            if let Some(grep) = &self.grep
                && !plain.to_lowercase().contains(grep)
            {
                continue;
            }
            lines.push(LogLine {
                level: self.level,
                text: if self.color { raw.to_string() } else { plain },
            });
        }
        Ok(lines)
    }
}

/// The log a `hearth logs` call reads: a session's, or the log a stopped session left.
fn log_source(
    ctx: &mut Ctx,
    name: Option<&str>,
) -> Result<(String, PathBuf, Option<Session>), CliError> {
    if let Some(name) = name {
        if let Some(session) = session::find(name) {
            return Ok((session.name.clone(), session.log.clone(), Some(session)));
        }
        let path = session::logs_dir().join(format!("{}.log", session::sanitize(name)));
        if path.is_file() {
            return Ok((name.to_string(), path, None));
        }
        return Err(unknown_session(name));
    }
    let session = pick_session(ctx, None)?;
    Ok((session.name.clone(), session.log.clone(), Some(session)))
}

/// `hearth logs`.
pub fn logs(ctx: &mut Ctx, args: LogsArgs) -> Result<Outcome, CliError> {
    let (name, path, session) = log_source(ctx, args.session.as_deref())?;
    let mut running = session.as_ref().is_some_and(Session::is_running);
    let mut reader = LogReader {
        path: path.clone(),
        cursor: args.since.unwrap_or(0),
        level: Level::Info,
        min_level: args.level,
        grep: args.grep.as_ref().map(|grep| grep.to_lowercase()),
        color: ctx.out.color,
    };
    let mut lines = reader.read(!running, &mut ctx.out)?;
    let total = lines.len();
    if let Some(tail) = args.tail {
        lines.drain(..lines.len().saturating_sub(tail));
    }
    let json_line = |line: &LogLine| json!({ "level": level_name(line.level), "text": line.text });

    if !args.follow {
        let human = if lines.is_empty() {
            ctx.out.paint("(nothing logged)", Style::Dim)
        } else {
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        if !running {
            ctx.out
                .info(ctx.out.paint(&format!("{name} isn't running"), Style::Dim));
        }
        return Ok(Outcome::new(
            json!({
                "session": name,
                "log": display_path(&path),
                "running": running,
                "cursor": reader.cursor,
                "count": lines.len(),
                "matched": total,
                "lines": lines.iter().map(json_line).collect::<Vec<_>>(),
            }),
            human,
        ));
    }

    let (_, duration) = ctx.stream_limits(None, args.duration);
    let deadline = duration.map(|duration| Instant::now() + duration);
    let mut emitted = 0u64;
    for line in &lines {
        let text = line.text.clone();
        ctx.out.event(json_line(line), || text);
        emitted += 1;
    }
    let mut checked = Instant::now();
    let stopped = loop {
        if !running {
            break "exited";
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            break "duration";
        }
        std::thread::sleep(Duration::from_millis(200));
        if checked.elapsed() >= Duration::from_secs(1) {
            running = session.as_ref().is_some_and(Session::is_running);
            checked = Instant::now();
        }
        for line in reader.read(!running, &mut ctx.out)? {
            let text = line.text.clone();
            ctx.out.event(json_line(&line), || text);
            emitted += 1;
        }
    };
    Ok(Outcome::new(
        json!({
            "session": name,
            "log": display_path(&path),
            "running": running,
            "cursor": reader.cursor,
            "count": emitted,
            "stopped": stopped,
        }),
        ctx.out
            .paint(&format!("{emitted} lines ({stopped})"), Style::Dim),
    ))
}

/// Waits until `done` returns true, checking a few times a second.
fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Stops a session's process. Returns how: `exited` when the app exited as asked over BRP,
/// `terminated` when it ended on a request from the system, or `killed`.
fn stop_session(session: &Session, force: bool) -> Result<&'static str, CliError> {
    if !session.is_running() {
        return Ok("already_exited");
    }
    if !force {
        let client = Client::new(Endpoint::localhost(session.port), Duration::from_secs(2));
        let asked = client
            .call(
                "world.write_message",
                Some(json!({ "message": known::APP_EXIT, "value": "Success" })),
            )
            .is_ok();
        if asked && wait_until(Duration::from_secs(5), || !session.is_running()) {
            return Ok("exited");
        }
        if session::terminate(session.pid, false).is_ok()
            && wait_until(Duration::from_secs(3), || !session.is_running())
        {
            return Ok("terminated");
        }
    }
    session::terminate(session.pid, true)?;
    if wait_until(Duration::from_secs(3), || !session.is_running()) {
        Ok("killed")
    } else {
        Err(CliError::new(
            "STOP_FAILED",
            Exit::Error,
            format!(
                "Process {} of session {} didn't end",
                session.pid, session.name
            ),
        ))
    }
}

/// `hearth stop`.
pub fn stop(ctx: &mut Ctx, args: StopArgs) -> Result<Outcome, CliError> {
    let sessions = if args.all {
        session::list()
    } else {
        vec![pick_session(ctx, args.session.as_deref())?]
    };
    if sessions.is_empty() {
        return Ok(Outcome::new(
            json!({ "stopped": [] }),
            "No sessions to stop",
        ));
    }
    let mut stopped = Vec::new();
    let mut lines = Vec::new();
    for session in &sessions {
        let how = stop_session(session, args.force)?;
        session.forget();
        lines.push(match how {
            "already_exited" => format!("{} had already exited", session.name),
            "exited" => format!("Stopped {} (it exited when asked)", session.name),
            "terminated" => format!("Stopped {} (ended by the system)", session.name),
            _ => format!("Stopped {} (killed)", session.name),
        });
        stopped.push(json!({
            "name": session.name,
            "pid": session.pid,
            "how": how,
            "log": display_path(&session.log),
        }));
    }
    Ok(Outcome::new(
        json!({ "stopped": stopped }),
        lines.join("\n"),
    ))
}

// --- hearth examples ---

/// `hearth examples`.
pub fn examples(ctx: &mut Ctx, args: ExamplesArgs) -> Result<Outcome, CliError> {
    let manifest = args.manifest_path.as_deref();
    let meta = project::metadata(manifest, project::toolchain(None).as_deref(), false)?;
    let package = select_package(&meta, args.package.as_deref(), manifest)?;
    let info = &package.metadata["example"];
    let grep = args.grep.as_ref().map(|grep| grep.to_lowercase());
    let category = args
        .category
        .as_ref()
        .map(|category| category.to_lowercase());
    let mut examples: Vec<Value> = package
        .targets_of("example")
        .into_iter()
        .map(|target| {
            let details = &info[&target.name];
            json!({
                "name": target.name,
                "category": details["category"],
                "description": details["description"],
                "required_features": target.required_features,
                "path": target
                    .src_path
                    .strip_prefix(package.dir())
                    .map(|path| path.display().to_string().replace('\\', "/"))
                    .unwrap_or_else(|_| display_path(&target.src_path)),
            })
        })
        .filter(|example| {
            let text = |key: &str| example[key].as_str().unwrap_or_default().to_lowercase();
            category
                .as_ref()
                .is_none_or(|category| text("category").contains(category))
                && grep.as_ref().is_none_or(|grep| {
                    text("name").contains(grep)
                        || text("category").contains(grep)
                        || text("description").contains(grep)
                })
        })
        .collect();
    examples.sort_by_key(|example| {
        (
            example["category"].as_str().unwrap_or("~").to_string(),
            example["name"].as_str().unwrap_or_default().to_string(),
        )
    });
    let rows: Vec<Vec<String>> = examples
        .iter()
        .map(|example| {
            let text = |key: &str| example[key].as_str().unwrap_or_default().to_string();
            let mut name = text("name");
            if let Some(features) = example["required_features"].as_array()
                && !features.is_empty()
            {
                name.push_str(&ctx.out.paint(" *", Style::Dim));
            }
            vec![
                name,
                text("category"),
                crate::value::shorten(&text("description"), 70),
            ]
        })
        .collect();
    let human = if rows.is_empty() {
        format!("{} has no examples that match", package.name)
    } else {
        format!(
            "{}\n{}",
            table(ctx.out.color, &["NAME", "CATEGORY", "DESCRIPTION"], &rows).trim_end(),
            ctx.out.paint(
                &format!(
                    "{} examples; * needs features, which `hearth run --example <name>` enables",
                    rows.len()
                ),
                Style::Dim
            )
        )
    };
    Ok(Outcome::new(Value::Array(examples), human))
}

// --- hearth doctor ---

/// The first line of `<program> --version`.
fn version_of(program: &str, toolchain: Option<&str>) -> Result<String, CliError> {
    let output = project::tool(program, toolchain)
        .arg("--version")
        .output()
        .map_err(|error| project::spawn_error(program, toolchain, &error))?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || text.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CliError::new(
            "CARGO_FAILED",
            Exit::Error,
            format!("`{program} --version` failed: {stderr}"),
        ));
    }
    Ok(text)
}

/// Whether a Rust file under `dir` contains `needle`, skipping build output and hidden folders.
fn sources_mention(dir: &Path, needle: &str) -> bool {
    let mut pending = vec![dir.to_path_buf()];
    let mut files = 0;
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !name.starts_with('.') && name != "target" {
                    pending.push(path);
                }
            } else if name.ends_with(".rs") {
                files += 1;
                if files > 5000 {
                    return true;
                }
                if std::fs::read_to_string(&path).is_ok_and(|text| text.contains(needle)) {
                    return true;
                }
            }
        }
    }
    false
}

/// `hearth doctor`.
pub fn doctor(ctx: &mut Ctx, args: DoctorArgs) -> Result<Outcome, CliError> {
    let toolchain = project::toolchain(args.toolchain.as_deref());
    let mut checks: Vec<Value> = Vec::new();
    let mut add = |name: &str, status: &str, message: String, hint: Option<String>| {
        checks.push(json!({ "name": name, "status": status, "message": message, "hint": hint }));
    };

    let rustc = version_of("rustc", toolchain.as_deref());
    match (&rustc, &version_of("cargo", toolchain.as_deref())) {
        (Ok(rustc), Ok(cargo)) => add("rust", "ok", format!("{rustc}; {cargo}"), None),
        (Err(error), _) | (_, Err(error)) => {
            add("rust", "error", error.message.clone(), error.hint.clone());
        }
    }

    let manifest = args.manifest_path.as_deref();
    match project::metadata(manifest, toolchain.as_deref(), true) {
        Ok(meta) => match select_package(&meta, None, manifest) {
            Ok(package) => {
                add(
                    "project",
                    "ok",
                    format!(
                        "{} {} in {}",
                        package.name,
                        package.version,
                        display_path(package.dir())
                    ),
                    None,
                );
                let bevy = meta.all.iter().find(|candidate| candidate.name == "bevy");
                let needs = [
                    package
                        .rust_version
                        .as_ref()
                        .map(|version| (version, package.name.as_str())),
                    bevy.and_then(|bevy| {
                        bevy.rust_version
                            .as_ref()
                            .map(|version| (version, "the engine"))
                    }),
                ]
                .into_iter()
                .flatten()
                .max_by_key(|(version, _)| version_numbers(version));
                let have = rustc
                    .as_ref()
                    .ok()
                    .and_then(|text| text.split_whitespace().nth(1));
                if let (Some((needed, who)), Some(have)) = (needs, have) {
                    if version_numbers(have) < version_numbers(needed) {
                        add(
                            "rust_version",
                            "error",
                            format!("rustc {have} is older than {needed}, which {who} needs"),
                            Some("Run `rustup update`, or build with a newer toolchain: --toolchain <name> or HEARTH_TOOLCHAIN (and --cargo-arg=--ignore-rust-version for a nightly of that version).".to_string()),
                        );
                    } else {
                        add(
                            "rust_version",
                            "ok",
                            format!("rustc {have} meets {who}'s {needed}"),
                            None,
                        );
                    }
                }
                match bevy {
                    Some(bevy) if package.name == "bevy" => add(
                        "bevy",
                        "info",
                        format!("This is the engine repository ({})", bevy.version),
                        Some("Examples that use BRP need `-F bevy_remote`; `hearth run --example <name>` enables the features an example requires.".to_string()),
                    ),
                    Some(bevy) => {
                        let features: Vec<String> = meta.resolve["nodes"]
                            .as_array()
                            .and_then(|nodes| {
                                nodes.iter().find(|node| {
                                    node["id"].as_str().is_some_and(|id| project::package_name(id) == "bevy")
                                })
                            })
                            .map(|node| {
                                node["features"]
                                    .as_array()
                                    .map(|features| features.iter().filter_map(Value::as_str).map(str::to_string).collect())
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default();
                        if features.iter().any(|feature| feature == "bevy_remote") {
                            add("bevy", "ok", format!("Engine {} with `bevy_remote`", bevy.version), None);
                            if !sources_mention(package.dir(), "RemotePlugin") {
                                add(
                                    "remote_plugin",
                                    "warn",
                                    "No source file adds `RemotePlugin`, which live commands need".to_string(),
                                    Some("Add `RemotePlugin` and `RemoteHttpPlugin` to the app; the template of `hearth new` shows how.".to_string()),
                                );
                            }
                        } else {
                            add(
                                "bevy",
                                "warn",
                                format!("Engine {} without the `bevy_remote` feature, so live commands can't reach the app", bevy.version),
                                Some("Enable `bevy_remote` on the bevy dependency, or add a `remote = [\"bevy/bevy_remote\"]` feature as `hearth new` projects do.".to_string()),
                            );
                        }
                    }
                    None => add(
                        "bevy",
                        "warn",
                        format!("{} doesn't depend on the engine (`bevy`)", package.name),
                        None,
                    ),
                }
            }
            Err(error) => add("project", "warn", error.message, error.hint),
        },
        Err(error) if error.code == "NO_PROJECT" => add(
            "project",
            "info",
            "No Cargo project here; `hearth build` and `hearth run` need one".to_string(),
            Some("`hearth new <name>` creates one.".to_string()),
        ),
        Err(error) => add("project", "error", error.message, error.hint),
    }

    let running: Vec<Session> = session::list()
        .into_iter()
        .filter(Session::is_running)
        .collect();
    let found = target::scan(&target::scan_ports());
    let apps: Vec<String> = found.iter().map(target::Found::describe).collect();
    add(
        "apps",
        if found.is_empty() && running.is_empty() {
            "info"
        } else {
            "ok"
        },
        format!(
            "{} running session{}, {} app{} answering BRP on ports {}-{}",
            running.len(),
            if running.len() == 1 { "" } else { "s" },
            found.len(),
            if found.len() == 1 { "" } else { "s" },
            DEFAULT_PORT,
            DEFAULT_PORT + 10
        ),
        (!apps.is_empty()).then(|| apps.join("; ")),
    );

    match ctx.target() {
        Ok(target) => {
            let endpoint = target.endpoint.to_string();
            match ctx.call("rpc.discover", Value::Null) {
                Ok(discover) => add(
                    "connection",
                    "ok",
                    format!(
                        "{endpoint} answers with {} BRP methods",
                        discover["methods"].as_array().map_or(0, Vec::len)
                    ),
                    None,
                ),
                Err(error) => add("connection", "info", error.message, error.hint),
            }
        }
        Err(error) => add("connection", "warn", error.message, error.hint),
    }

    match crate::skill::installed(SkillClient::Claude) {
        Some(path) => add("skill", "ok", format!("The agent skill is installed at {}", display_path(&path)), None),
        None => add(
            "skill",
            "info",
            "The agent skill isn't installed for Claude Code".to_string(),
            Some("`hearth skill install` teaches agents to use this CLI (`codex` and `agents` install for others).".to_string()),
        ),
    }

    let ok = !checks.iter().any(|check| check["status"] == "error");
    let width = checks
        .iter()
        .filter_map(|check| check["name"].as_str())
        .map(str::len)
        .max()
        .unwrap_or(0);
    let mut lines = Vec::new();
    for check in &checks {
        let (mark, style) = match check["status"].as_str().unwrap_or_default() {
            "ok" => ("✓", Style::Type),
            "warn" => ("!", Style::Warn),
            "error" => ("✗", Style::Error),
            _ => ("·", Style::Dim),
        };
        lines.push(format!(
            "{} {:<width$}  {}",
            ctx.out.paint(mark, style),
            check["name"].as_str().unwrap_or_default(),
            check["message"].as_str().unwrap_or_default()
        ));
        if let Some(hint) = check["hint"].as_str() {
            lines.push(format!(
                "  {:<width$}  {}",
                "",
                ctx.out.paint(hint, Style::Dim)
            ));
        }
    }
    Ok(Outcome::new(
        json!({ "ok": ok, "checks": checks }),
        lines.join("\n"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_names() {
        assert!(validate_name("my_game").is_ok());
        assert!(validate_name("space-shooter2").is_ok());
        assert!(validate_name("2fast").is_err());
        assert!(validate_name("my game").is_err());
        assert!(validate_name("bevy").is_err());
        assert!(validate_name("").is_err());
    }

    #[test]
    fn titles_names() {
        assert_eq!(title_of("my_game"), "My Game");
        assert_eq!(title_of("space-shooter"), "Space Shooter");
        assert_eq!(title_of("x"), "X");
    }

    #[test]
    fn writes_toml_strings() {
        assert_eq!(toml_string("C:/bevy"), "\"C:/bevy\"");
        assert_eq!(toml_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn reads_log_levels() {
        assert_eq!(
            line_level("2025-01-01T12:00:00.000000Z  WARN bevy_render: slow"),
            Some(Level::Warn)
        );
        assert_eq!(
            line_level("2025-01-01T12:00:00Z ERROR app: boom"),
            Some(Level::Error)
        );
        assert_eq!(
            line_level("thread 'main' panicked at src/main.rs:3:5:"),
            Some(Level::Error)
        );
        assert_eq!(line_level("plain output"), None);
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration_text(45), "45s");
        assert_eq!(duration_text(600), "10m");
        assert_eq!(duration_text(3 * 3600 + 120), "3h 2m");
        assert_eq!(duration_text(2 * 86400 + 3600), "2d 1h");
    }
}
