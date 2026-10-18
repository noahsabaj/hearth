//! The command line: every command, argument and flag, with the help text people and agents read.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::output::{ColorChoice, Format};

/// Build, run, inspect and drive Hearth apps.
///
/// Live-app commands talk to a running app over the Hearth Remote Protocol (HRP): the app needs
/// the engine's `hearth_remote` feature with `RemotePlugin` and `RemoteHttpPlugin` added. Every command
/// prints structured JSON with `--json`, and `hearth commands` lists them all for scripts and agents.
#[derive(Parser, Debug)]
#[command(
    name = "hearth",
    version,
    propagate_version = true,
    max_term_width = 100,
    after_help = "Run `hearth <command> --help` for details, `hearth commands --grep <pattern>` to search,\nand `hearth skill show` for the agent guide."
)]
pub struct Cli {
    /// Options every command accepts.
    #[command(flatten)]
    pub global: GlobalArgs,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Options every command accepts, before or after the command name.
#[derive(Args, Debug, Clone)]
#[command(next_help_heading = "Global options")]
pub struct GlobalArgs {
    /// The app to talk to: a session name, a port, `host:port` or a URL. Defaults to the
    /// session started from this project, else 127.0.0.1:15702. Also read from `HEARTH_APP`.
    #[arg(long, global = true, value_name = "APP")]
    pub app: Option<String>,

    /// Output format. Also read from `HEARTH_FORMAT`.
    #[arg(long, global = true, value_enum, value_name = "FORMAT")]
    pub format: Option<Format>,

    /// Shorthand for `--format json`.
    #[arg(long, global = true)]
    pub json: bool,

    /// Seconds to wait for the app to answer a request, 10 by default. Also read from
    /// `HEARTH_TIMEOUT`.
    #[arg(long, global = true, value_name = "SECONDS")]
    pub timeout: Option<f64>,

    /// Print less to stderr: no progress, only warnings and errors.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// When to color human output.
    #[arg(long, global = true, value_enum, value_name = "WHEN", default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Shorthand for `--color never`.
    #[arg(long, global = true)]
    pub no_color: bool,
}

/// All commands, grouped in the help by [`group_of`].
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create a new Hearth project, ready for `hearth run`.
    #[command(
        after_help = "Examples:\n  hearth new my_game\n  hearth new my_game --template 2d --path ~/games\n  hearth new my_game --engine-path ../hearth"
    )]
    New(NewArgs),

    /// Build the project with cargo and report diagnostics.
    #[command(
        after_help = "Examples:\n  hearth build\n  hearth build --release --example breakout\n  hearth build --json   # errors and warnings as structured diagnostics"
    )]
    Build(BuildArgs),

    /// Type-check the project with cargo and report diagnostics, without producing binaries.
    #[command(after_help = "Examples:\n  hearth check\n  hearth check -F hearth_remote --json")]
    Check(BuildArgs),

    /// Build and run the app, attached to the terminal or detached as a session.
    #[command(
        after_help = "Examples:\n  hearth run\n  hearth run --detach                       # returns once the app answers over HRP\n  hearth run --example server -F hearth_remote --detach\n  hearth run --release -- --my-app-flag     # arguments after -- go to the app"
    )]
    Run(RunArgs),

    /// Build and open the Hearth editor for the project, with the game's code linked in.
    #[command(
        after_help = "Examples:\n  hearth editor\n  hearth editor --detach                    # returns once the editor answers over HRP\n  hearth editor --scene assets/scenes/level.scn.ron\n  hearth editor -p my_game --release\n\nThe editor is a crate generated in the project's target directory, at `hearth-editor/`. It depends\non the project's engine with the `hearth_editor` feature and on the project's library, so it knows\nthe game's types, and builds into the project's target directory, sharing the engine build. When\n`Cargo.toml` names a function of the library in `[package.metadata.hearth] editor-hook`, such as\n`editor-hook = \"register_types\"`, the editor calls it with its `App` at startup."
    )]
    Editor(EditorArgs),

    /// List running apps: sessions started with `hearth run --detach` and other apps found on
    /// the usual ports.
    #[command(
        visible_alias = "sessions",
        after_help = "Examples:\n  hearth ps\n  hearth ps --json"
    )]
    Ps(PsArgs),

    /// Show the log of a session started with `hearth run --detach`.
    #[command(
        after_help = "Examples:\n  hearth logs\n  hearth logs --tail 50 --level warn\n  hearth logs --since 18234        # only what was logged after an earlier call's cursor\n  hearth logs --follow"
    )]
    Logs(LogsArgs),

    /// Stop a session: ask the app to exit, then end the process if it doesn't.
    #[command(after_help = "Examples:\n  hearth stop\n  hearth stop my_game\n  hearth stop --all")]
    Stop(StopArgs),

    /// List the project's examples.
    #[command(
        after_help = "Examples:\n  hearth examples\n  hearth examples --grep ui\n  hearth examples --category \"3D Rendering\""
    )]
    Examples(ExamplesArgs),

    /// Check the toolchain, the project and the connection to a running app.
    Doctor(DoctorArgs),

    /// Show the app's name, engine version, entity counts, time and window.
    #[command(
        after_help = "Examples:\n  hearth status\n  hearth status --app 15703   # the render sub-app"
    )]
    Status,

    /// Show the entity hierarchy, with names and ids.
    #[command(
        after_help = "Examples:\n  hearth tree\n  hearth tree Player --depth 2\n  hearth tree --components\n  hearth tree --all          # include observers, systems and resource entities"
    )]
    Tree(TreeArgs),

    /// Find entities by name or by the components they have.
    #[command(
        after_help = "Examples:\n  hearth find player\n  hearth find 'Enemy*'\n  hearth find --with Camera3d\n  hearth find --with Mesh3d --without Visibility"
    )]
    Find(FindArgs),

    /// Show an entity's components and their values.
    #[command(
        after_help = "Examples:\n  hearth get Player\n  hearth get 495v0 Transform\n  hearth get Player Transform.translation.y\n  hearth get Camera Transform Projection --json"
    )]
    Get(GetArgs),

    /// Change a component or one of its fields on an entity.
    #[command(
        after_help = "Examples:\n  hearth set Player Transform.translation '[0, 2, 0]'\n  hearth set Player Transform.translation.y 2\n  hearth set Player Transform '{\"scale\": [2, 2, 2]}'   # fields not given keep their values\n  hearth set Player Name Bob\n  hearth set Player Transform @transform.json"
    )]
    Set(SetArgs),

    /// Add a component to an entity, or replace it.
    #[command(
        after_help = "Examples:\n  hearth insert Player Visibility Hidden\n  hearth insert 495v0 my_game::Health '{\"current\": 10, \"max\": 10}'\n  hearth insert Player my_game::Frozen      # a marker: its default value"
    )]
    Insert(InsertArgs),

    /// Remove components from an entity.
    #[command(after_help = "Examples:\n  hearth remove Player my_game::Frozen")]
    Remove(RemoveArgs),

    /// Spawn an entity.
    #[command(
        after_help = "Examples:\n  hearth spawn --name Marker Transform\n  hearth spawn 'Transform={\"translation\": [0, 1, 0]}' Visibility --parent Player\n  hearth spawn --bundle '{\"hearth_ecs::name::Name\": \"Probe\"}'"
    )]
    Spawn(SpawnArgs),

    /// Despawn entities and their descendants.
    #[command(after_help = "Examples:\n  hearth despawn Enemy\n  hearth despawn 512v0 513v0")]
    Despawn(DespawnArgs),

    /// Change the parent of entities.
    #[command(
        after_help = "Examples:\n  hearth reparent Sword --parent Player\n  hearth reparent Sword --root"
    )]
    Reparent(ReparentArgs),

    /// Query entities by components and print the component values.
    #[command(
        after_help = "Examples:\n  hearth query Transform\n  hearth query Transform --with my_game::Enemy\n  hearth query Name --option Transform --without Camera\n  hearth query Transform --limit 5 --json"
    )]
    Query(QueryArgs),

    /// List, read and change resources.
    #[command(subcommand)]
    Resource(ResourceCommand),

    /// Search the types the app registered for reflection.
    #[command(
        after_help = "Examples:\n  hearth types transform\n  hearth types --kind resource\n  hearth types --crate my_game"
    )]
    Types(TypesArgs),

    /// Show a type's fields, variants and an example value to start from.
    #[command(
        after_help = "Examples:\n  hearth schema Transform\n  hearth schema Visibility --json"
    )]
    Schema(SchemaArgs),

    /// Stream changes to an entity's components.
    #[command(
        after_help = "Examples:\n  hearth watch Player Transform --count 5\n  hearth watch Player --duration 10      # components added and removed\n  hearth watch Player Transform --format ndjson"
    )]
    Watch(WatchArgs),

    /// Stream events triggered in the app.
    #[command(
        after_help = "Examples:\n  hearth observe my_game::Scored --count 1\n  hearth observe Pointer<Click> --entity Button --duration 30"
    )]
    Observe(ObserveArgs),

    /// Trigger an event in the app.
    #[command(after_help = "Examples:\n  hearth trigger my_game::Explode '{\"radius\": 4}'")]
    Trigger(TriggerArgs),

    /// Write a message for the app's systems to read.
    #[command(
        after_help = "Examples:\n  hearth send AppExit Success\n  hearth send my_game::SpawnWave '{\"count\": 3}'"
    )]
    Send(SendArgs),

    /// Capture the app's window to a PNG file.
    #[command(
        after_help = "Examples:\n  hearth screenshot\n  hearth screenshot --out shot.png\n  hearth screenshot --max-size 800"
    )]
    Screenshot(ScreenshotArgs),

    /// Send keyboard and mouse input to the app's window.
    #[command(subcommand)]
    Input(InputCommand),

    /// Show, pause, resume or speed up the app's virtual time.
    #[command(
        after_help = "Examples:\n  hearth time\n  hearth time pause\n  hearth time speed 0.25\n  hearth time resume"
    )]
    Time(TimeArgs),

    /// List schedules, or show the systems of one.
    #[command(after_help = "Examples:\n  hearth schedules\n  hearth schedules Update")]
    Schedules(SchedulesArgs),

    /// Show diagnostics such as frame time and entity count.
    #[command(after_help = "Examples:\n  hearth diagnostics\n  hearth diagnostics fps")]
    Diagnostics(DiagnosticsArgs),

    /// List the HRP methods the app supports.
    Methods,

    /// Call any HRP method with raw JSON parameters.
    #[command(
        after_help = "Examples:\n  hearth call world.list_resources\n  hearth call world.get_components '{\"entity\": \"495v0\", \"components\": [\"hearth_ecs::name::Name\"]}'\n  hearth call world.get_components+watch @params.json --count 3"
    )]
    Call(CallArgs),

    /// Ask the app to exit.
    #[command(after_help = "Examples:\n  hearth exit\n  hearth exit --code 3")]
    Exit(ExitArgs),

    /// List every command with its arguments, as text or as JSON for scripts and agents.
    #[command(
        after_help = "Examples:\n  hearth commands\n  hearth commands --grep entity\n  hearth commands --json"
    )]
    Commands(CommandsArgs),

    /// Show or install the agent skill: how agents should use this CLI.
    #[command(subcommand)]
    Skill(SkillCommand),

    /// Serve the commands as tools over the Model Context Protocol, on stdio.
    #[command(
        after_help = "Examples:\n  claude mcp add hearth -- hearth mcp\n  claude mcp add hearth -- hearth mcp --app my_game"
    )]
    Mcp(McpArgs),
}

/// The help section a top-level command is listed in.
pub fn group_of(command: &str) -> &'static str {
    match command {
        "new" | "build" | "check" | "run" | "editor" | "ps" | "logs" | "stop" | "examples"
        | "doctor" => "Project",
        "commands" | "skill" | "mcp" | "help" => "Agents and scripting",
        _ => "Live app (over HRP)",
    }
}

/// The order of the help sections.
pub const GROUPS: &[&str] = &["Project", "Live app (over HRP)", "Agents and scripting"];

/// Templates for `hearth new`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Template {
    /// A 3D scene with a camera, a light and a cube.
    #[value(name = "3d")]
    ThreeD,
    /// A 2D scene with a camera and a sprite.
    #[value(name = "2d")]
    TwoD,
    /// A window and a camera, nothing else.
    Minimal,
}

/// Arguments of `hearth new`.
#[derive(Args, Debug)]
pub struct NewArgs {
    /// The project name, which is also its directory name.
    pub name: String,

    /// The directory to create the project in.
    #[arg(long, value_name = "DIR", default_value = ".")]
    pub path: PathBuf,

    /// What the project starts with.
    #[arg(long, value_enum, default_value_t = Template::ThreeD)]
    pub template: Template,

    /// Depend on a local checkout of the engine instead of Hearth's git repository.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["engine_git", "engine_branch"])]
    pub engine_path: Option<PathBuf>,

    /// Depend on another git repository of the engine, such as your own fork.
    #[arg(long, value_name = "URL")]
    pub engine_git: Option<String>,

    /// The branch of the engine's git repository.
    #[arg(long, value_name = "BRANCH")]
    pub engine_branch: Option<String>,

    /// Don't initialize a git repository.
    #[arg(long)]
    pub no_git: bool,

    /// Write into the directory even if it isn't empty.
    #[arg(long)]
    pub force: bool,
}

/// Cargo options shared by `build`, `check` and `run`.
#[derive(Args, Debug, Clone, Default)]
pub struct CargoArgs {
    /// The package to build, in a workspace.
    #[arg(short, long, value_name = "SPEC")]
    pub package: Option<String>,

    /// Build this binary target.
    #[arg(long, value_name = "NAME", conflicts_with = "example")]
    pub bin: Option<String>,

    /// Build this example.
    #[arg(long, value_name = "NAME")]
    pub example: Option<String>,

    /// Build with the release profile.
    #[arg(short, long, conflicts_with = "profile")]
    pub release: bool,

    /// Build with this profile.
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,

    /// Cargo features to enable, separated by commas or repeated.
    #[arg(short = 'F', long, value_name = "FEATURES", value_delimiter = ',')]
    pub features: Vec<String>,

    /// Enable all features.
    #[arg(long)]
    pub all_features: bool,

    /// Disable the default features.
    #[arg(long)]
    pub no_default_features: bool,

    /// The project's `Cargo.toml`, when not in the current directory.
    #[arg(long, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// The rustup toolchain to build with, such as `nightly`. Also read from `HEARTH_TOOLCHAIN`.
    #[arg(long, value_name = "NAME")]
    pub toolchain: Option<String>,

    /// Extra arguments for cargo, repeated for each one, such as `--cargo-arg=--locked`.
    #[arg(long = "cargo-arg", value_name = "ARG", allow_hyphen_values = true)]
    pub cargo_args: Vec<String>,
}

/// Arguments of `hearth build` and `hearth check`.
#[derive(Args, Debug)]
pub struct BuildArgs {
    /// Cargo options.
    #[command(flatten)]
    pub cargo: CargoArgs,
}

/// Arguments of `hearth run`.
#[derive(Args, Debug)]
pub struct RunArgs {
    /// Cargo options.
    #[command(flatten)]
    pub cargo: CargoArgs,

    /// Run in the background as a session; logs go to a file you read with `hearth logs`.
    #[arg(short, long)]
    pub detach: bool,

    /// The session name. Defaults to the binary or example name.
    #[arg(long, value_name = "NAME", requires = "detach")]
    pub name: Option<String>,

    /// The HRP port, passed to the app as `HEARTH_REMOTE_PORT`. Defaults to 15702, or the next
    /// free port when another app uses it.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// Don't wait for the app to answer over HRP before returning.
    #[arg(long, requires = "detach")]
    pub no_wait: bool,

    /// Seconds to wait for the app to answer over HRP.
    #[arg(long, value_name = "SECONDS", default_value_t = 120.0)]
    pub wait_timeout: f64,

    /// Arguments for the app, after `--`.
    #[arg(last = true, value_name = "ARGS")]
    pub args: Vec<String>,
}

/// Arguments of `hearth editor`.
#[derive(Args, Debug)]
pub struct EditorArgs {
    /// The package whose project to edit, in a workspace.
    #[arg(short, long, value_name = "SPEC")]
    pub package: Option<String>,

    /// Build the editor with the release profile.
    #[arg(short, long, conflicts_with = "profile")]
    pub release: bool,

    /// Build the editor with this profile.
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,

    /// Features of the project's package to enable in the editor, separated by commas or
    /// repeated.
    #[arg(short = 'F', long, value_name = "FEATURES", value_delimiter = ',')]
    pub features: Vec<String>,

    /// The project's `Cargo.toml`, when not in the current directory.
    #[arg(long, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// The rustup toolchain to build with, such as `nightly`. Also read from `HEARTH_TOOLCHAIN`.
    #[arg(long, value_name = "NAME")]
    pub toolchain: Option<String>,

    /// Extra arguments for cargo, repeated for each one, such as `--cargo-arg=--locked`.
    #[arg(long = "cargo-arg", value_name = "ARG", allow_hyphen_values = true)]
    pub cargo_args: Vec<String>,

    /// The scene to open, relative to the current directory. The editor gets its full path in
    /// `HEARTH_EDITOR_SCENE`.
    #[arg(long, value_name = "PATH")]
    pub scene: Option<PathBuf>,

    /// Run in the background as a session; logs go to a file you read with `hearth logs`.
    #[arg(short, long)]
    pub detach: bool,

    /// The session name. Defaults to `editor`.
    #[arg(long, value_name = "NAME", requires = "detach")]
    pub name: Option<String>,

    /// The HRP port, passed to the editor as `HEARTH_REMOTE_PORT`. Defaults to 15702, or the next
    /// free port when another app uses it.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// Don't wait for the editor to answer over HRP before returning.
    #[arg(long, requires = "detach")]
    pub no_wait: bool,

    /// Seconds to wait for the editor to answer over HRP.
    #[arg(long, value_name = "SECONDS", default_value_t = 120.0)]
    pub wait_timeout: f64,
}

/// Arguments of `hearth ps`.
#[derive(Args, Debug)]
pub struct PsArgs {
    /// Don't scan the usual ports for apps that weren't started with `hearth run --detach`.
    #[arg(long)]
    pub no_scan: bool,
}

/// Log levels, from the most to the least verbose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Level {
    /// Everything.
    Trace,
    /// Debug and above.
    Debug,
    /// Info and above.
    Info,
    /// Warnings and errors.
    Warn,
    /// Errors only.
    Error,
}

/// Arguments of `hearth logs`.
#[derive(Args, Debug)]
pub struct LogsArgs {
    /// The session. Defaults to the session of `--app` or of this project.
    pub session: Option<String>,

    /// Keep printing lines as the app logs them.
    #[arg(short, long)]
    pub follow: bool,

    /// Start at this cursor, as returned by an earlier call, to read only newer lines.
    #[arg(long, value_name = "CURSOR")]
    pub since: Option<u64>,

    /// Show only the last N lines.
    #[arg(short = 'n', long, value_name = "N")]
    pub tail: Option<usize>,

    /// Show only lines at this level or above.
    #[arg(short, long, value_enum)]
    pub level: Option<Level>,

    /// Show only lines containing this text (case-insensitive).
    #[arg(short, long, value_name = "TEXT")]
    pub grep: Option<String>,

    /// With `--follow`, stop after this many seconds.
    #[arg(long, value_name = "SECONDS")]
    pub duration: Option<f64>,
}

/// Arguments of `hearth stop`.
#[derive(Args, Debug)]
pub struct StopArgs {
    /// The session. Defaults to the session of `--app` or of this project.
    pub session: Option<String>,

    /// Stop every session.
    #[arg(long, conflicts_with = "session")]
    pub all: bool,

    /// End the process right away instead of asking the app to exit.
    #[arg(long)]
    pub force: bool,
}

/// Arguments of `hearth examples`.
#[derive(Args, Debug)]
pub struct ExamplesArgs {
    /// Show only examples whose name, category or description contains this text.
    #[arg(short, long, value_name = "TEXT")]
    pub grep: Option<String>,

    /// Show only examples in this category.
    #[arg(long, value_name = "CATEGORY")]
    pub category: Option<String>,

    /// The package whose examples to list, in a workspace.
    #[arg(short, long, value_name = "NAME")]
    pub package: Option<String>,

    /// The project's `Cargo.toml`, when not in the current directory.
    #[arg(long, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,
}

/// Arguments of `hearth doctor`.
#[derive(Args, Debug)]
pub struct DoctorArgs {
    /// The project's `Cargo.toml`, when not in the current directory.
    #[arg(long, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// The rustup toolchain the project builds with. Also read from `HEARTH_TOOLCHAIN`.
    #[arg(long, value_name = "NAME")]
    pub toolchain: Option<String>,
}

/// Arguments of `hearth tree`.
#[derive(Args, Debug)]
pub struct TreeArgs {
    /// Show only this entity and its descendants.
    pub root: Option<String>,

    /// How many levels of children to show.
    #[arg(long, value_name = "N")]
    pub depth: Option<usize>,

    /// List each entity's components.
    #[arg(short, long)]
    pub components: bool,

    /// Include the engine's internal entities: observers, systems and resources.
    #[arg(short, long)]
    pub all: bool,
}

/// Arguments of `hearth find`.
#[derive(Args, Debug)]
pub struct FindArgs {
    /// Text the name contains, case-insensitive; `*` and `?` match any text and any character.
    pub pattern: Option<String>,

    /// Only entities with this component.
    #[arg(short, long, value_name = "COMPONENT")]
    pub with: Vec<String>,

    /// Only entities without this component.
    #[arg(long, value_name = "COMPONENT")]
    pub without: Vec<String>,

    /// Include the engine's internal entities.
    #[arg(short, long)]
    pub all: bool,

    /// Show at most this many entities.
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,
}

/// Arguments of `hearth get`.
#[derive(Args, Debug)]
pub struct GetArgs {
    /// The entity: an id such as `495v0`, or a name.
    pub entity: String,

    /// The components to show, each optionally with a field path such as
    /// `Transform.translation`. Defaults to all of them.
    pub components: Vec<String>,

    /// Print values in full instead of shortening long ones.
    #[arg(long)]
    pub full: bool,
}

/// Arguments of `hearth set`.
#[derive(Args, Debug)]
pub struct SetArgs {
    /// The entity: an id such as `495v0`, or a name.
    pub entity: String,

    /// The component, optionally followed by a field path such as `Transform.translation.x`.
    pub target: String,

    /// The value as JSON, a bare word for a string, `@file` for a JSON file or `-` for stdin.
    #[arg(allow_hyphen_values = true)]
    pub value: String,
}

/// Arguments of `hearth insert`.
#[derive(Args, Debug)]
pub struct InsertArgs {
    /// The entity: an id such as `495v0`, or a name.
    pub entity: String,

    /// The component.
    pub component: String,

    /// The value as JSON, a bare word, `@file` or `-`. Defaults to the component's default value.
    #[arg(allow_hyphen_values = true)]
    pub value: Option<String>,
}

/// Arguments of `hearth remove`.
#[derive(Args, Debug)]
pub struct RemoveArgs {
    /// The entity: an id such as `495v0`, or a name.
    pub entity: String,

    /// The components to remove.
    #[arg(required = true)]
    pub components: Vec<String>,
}

/// Arguments of `hearth spawn`.
#[derive(Args, Debug)]
pub struct SpawnArgs {
    /// Components as `Component` for the default value or `Component=VALUE`.
    pub components: Vec<String>,

    /// Give the entity a `Name`.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,

    /// Make the entity a child of this entity.
    #[arg(long, value_name = "ENTITY")]
    pub parent: Option<String>,

    /// All components as one JSON object of type paths to values, `@file` or `-`.
    #[arg(long, value_name = "JSON")]
    pub bundle: Option<String>,
}

/// Arguments of `hearth despawn`.
#[derive(Args, Debug)]
pub struct DespawnArgs {
    /// The entities: ids such as `495v0`, or names.
    #[arg(required = true)]
    pub entities: Vec<String>,
}

/// Arguments of `hearth reparent`.
#[derive(Args, Debug)]
pub struct ReparentArgs {
    /// The entities to move.
    #[arg(required = true)]
    pub entities: Vec<String>,

    /// The new parent.
    #[arg(
        long,
        value_name = "ENTITY",
        required_unless_present = "root",
        conflicts_with = "root"
    )]
    pub parent: Option<String>,

    /// Make the entities roots, without a parent.
    #[arg(long)]
    pub root: bool,
}

/// Arguments of `hearth query`.
#[derive(Args, Debug)]
pub struct QueryArgs {
    /// Components every entity must have; their values are printed.
    pub components: Vec<String>,

    /// Components to print when present.
    #[arg(short, long, value_name = "COMPONENT")]
    pub option: Vec<String>,

    /// Components to report as present or absent, without their values.
    #[arg(long, value_name = "COMPONENT")]
    pub has: Vec<String>,

    /// Only entities with this component.
    #[arg(short, long, value_name = "COMPONENT")]
    pub with: Vec<String>,

    /// Only entities without this component.
    #[arg(long, value_name = "COMPONENT")]
    pub without: Vec<String>,

    /// Include the engine's internal entities.
    #[arg(short, long)]
    pub all: bool,

    /// Show at most this many entities.
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,

    /// Print values in full instead of shortening long ones.
    #[arg(long)]
    pub full: bool,
}

/// The `hearth resource` commands.
#[derive(Subcommand, Debug)]
pub enum ResourceCommand {
    /// List the resources in the app.
    #[command(
        after_help = "Examples:\n  hearth resource list\n  hearth resource list --grep time"
    )]
    List {
        /// Show only resources whose type contains this text (case-insensitive).
        #[arg(short, long, value_name = "TEXT")]
        grep: Option<String>,
    },
    /// Show a resource's value.
    #[command(
        after_help = "Examples:\n  hearth resource get Time<Virtual>\n  hearth resource get ClearColor --json"
    )]
    Get {
        /// The resource.
        resource: String,
    },
    /// Change a resource or one of its fields.
    #[command(
        after_help = "Examples:\n  hearth resource set ClearColor '{\"Srgba\": {\"red\": 0.1, \"green\": 0.1, \"blue\": 0.2, \"alpha\": 1}}'\n  hearth resource set Time<Virtual>.context.relative_speed 0.5"
    )]
    Set {
        /// The resource, optionally followed by a field path.
        target: String,
        /// The value as JSON, a bare word, `@file` or `-`.
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
    /// Add a resource, or replace it.
    #[command(after_help = "Examples:\n  hearth resource insert my_game::Score '{\"value\": 0}'")]
    Insert {
        /// The resource.
        resource: String,
        /// The value as JSON, a bare word, `@file` or `-`. Defaults to its default value.
        #[arg(allow_hyphen_values = true)]
        value: Option<String>,
    },
    /// Remove a resource.
    #[command(after_help = "Examples:\n  hearth resource remove my_game::Score")]
    Remove {
        /// The resource.
        resource: String,
    },
}

/// What kind of types `hearth types` lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum TypeKind {
    /// Components.
    Component,
    /// Resources.
    Resource,
    /// Events, for `hearth trigger` and `hearth observe`.
    Event,
    /// Every reflected type.
    All,
}

/// Arguments of `hearth types`.
#[derive(Args, Debug)]
pub struct TypesArgs {
    /// Text the type path contains, case-insensitive.
    pub pattern: Option<String>,

    /// Show only types of this kind.
    #[arg(short, long, value_enum, default_value_t = TypeKind::All)]
    pub kind: TypeKind,

    /// Show only types from this crate.
    #[arg(long = "crate", value_name = "NAME")]
    pub crate_name: Option<String>,

    /// Show at most this many types.
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,
}

/// Arguments of `hearth schema`.
#[derive(Args, Debug)]
pub struct SchemaArgs {
    /// The type: a full type path or a short name such as `Transform`.
    pub type_name: String,
}

/// Options for commands that stream events.
#[derive(Args, Debug, Clone, Default)]
pub struct StreamArgs {
    /// Stop after this many events.
    #[arg(short = 'n', long, value_name = "N")]
    pub count: Option<u64>,

    /// Stop after this many seconds.
    #[arg(short, long, value_name = "SECONDS")]
    pub duration: Option<f64>,
}

/// Arguments of `hearth watch`.
#[derive(Args, Debug)]
pub struct WatchArgs {
    /// The entity: an id such as `495v0`, or a name.
    pub entity: String,

    /// The components to watch. Without any, reports components added and removed.
    pub components: Vec<String>,

    /// When to stop.
    #[command(flatten)]
    pub stream: StreamArgs,
}

/// Arguments of `hearth observe`.
#[derive(Args, Debug)]
pub struct ObserveArgs {
    /// The event type.
    pub event: String,

    /// Only events targeting this entity.
    #[arg(short, long, value_name = "ENTITY")]
    pub entity: Option<String>,

    /// When to stop.
    #[command(flatten)]
    pub stream: StreamArgs,
}

/// Arguments of `hearth trigger`.
#[derive(Args, Debug)]
pub struct TriggerArgs {
    /// The event type.
    pub event: String,

    /// The event value as JSON, a bare word, `@file` or `-`.
    #[arg(allow_hyphen_values = true)]
    pub value: Option<String>,
}

/// Arguments of `hearth send`.
#[derive(Args, Debug)]
pub struct SendArgs {
    /// The message type.
    pub message: String,

    /// The message value as JSON, a bare word, `@file` or `-`.
    #[arg(allow_hyphen_values = true)]
    pub value: Option<String>,
}

/// Arguments of `hearth screenshot`.
#[derive(Args, Debug)]
pub struct ScreenshotArgs {
    /// Where to write the PNG. Defaults to a new file in the system's temporary directory.
    #[arg(short, long, value_name = "PATH")]
    pub out: Option<PathBuf>,

    /// Scale the image down so that neither side exceeds this many pixels.
    #[arg(long, value_name = "PIXELS")]
    pub max_size: Option<u32>,

    /// The window entity. Defaults to the primary window.
    #[arg(long, value_name = "ENTITY")]
    pub window: Option<String>,
}

/// Mouse buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Button {
    /// The left button.
    Left,
    /// The right button.
    Right,
    /// The middle button.
    Middle,
    /// The back button.
    Back,
    /// The forward button.
    Forward,
}

/// The `hearth input` commands.
#[derive(Subcommand, Debug)]
pub enum InputCommand {
    /// Press and release keys, together as a chord.
    #[command(
        after_help = "Examples:\n  hearth input key Space\n  hearth input key w --hold 2000\n  hearth input key ctrl+s\n  hearth input key Escape --json\n\nKeys are `KeyCode` names (KeyA, Digit1, ArrowUp, F5) or aliases: a-z, 0-9, space,\nenter, esc, tab, backspace, up, down, left, right, shift, ctrl, alt and super."
    )]
    Key {
        /// The keys, pressed in order and released in reverse. `ctrl+s` is the same as `ctrl s`.
        #[arg(required = true)]
        keys: Vec<String>,
        /// Milliseconds to hold the keys down.
        #[arg(long, value_name = "MS", default_value_t = 60)]
        hold: u64,
        /// Only press the keys, leaving them down.
        #[arg(long, conflicts_with = "up")]
        down: bool,
        /// Only release the keys.
        #[arg(long)]
        up: bool,
    },
    /// Type text, one key per character.
    #[command(after_help = "Examples:\n  hearth input type 'hello world'")]
    Type {
        /// The text.
        text: String,
    },
    /// Move the cursor, in logical pixels from the window's top-left corner.
    #[command(after_help = "Examples:\n  hearth input move 640 360")]
    Move {
        /// Pixels from the left edge.
        #[arg(allow_negative_numbers = true)]
        x: f64,
        /// Pixels from the top edge.
        #[arg(allow_negative_numbers = true)]
        y: f64,
    },
    /// Click a mouse button, at a position or where the cursor is.
    #[command(
        after_help = "Examples:\n  hearth input click 640 360\n  hearth input click --button right\n  hearth input click 100 40 --hold 500"
    )]
    Click {
        /// Pixels from the left edge.
        #[arg(requires = "y", allow_negative_numbers = true)]
        x: Option<f64>,
        /// Pixels from the top edge.
        #[arg(allow_negative_numbers = true)]
        y: Option<f64>,
        /// The button.
        #[arg(short, long, value_enum, default_value_t = Button::Left)]
        button: Button,
        /// Milliseconds to hold the button down.
        #[arg(long, value_name = "MS", default_value_t = 60)]
        hold: u64,
        /// Click twice.
        #[arg(long)]
        double: bool,
    },
    /// Scroll the mouse wheel.
    #[command(
        after_help = "Examples:\n  hearth input scroll -3\n  hearth input scroll 120 --pixels"
    )]
    Scroll {
        /// Vertical lines (or pixels) to scroll; positive scrolls up.
        #[arg(allow_negative_numbers = true)]
        y: f32,
        /// Horizontal lines (or pixels) to scroll.
        #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
        x: f32,
        /// Scroll by pixels instead of lines.
        #[arg(long)]
        pixels: bool,
    },
}

/// Arguments of `hearth time`.
#[derive(Args, Debug)]
pub struct TimeArgs {
    /// What to do. Without one, shows the time.
    #[command(subcommand)]
    pub action: Option<TimeCommand>,
}

/// The `hearth time` commands.
#[derive(Subcommand, Debug, Clone, Copy)]
pub enum TimeCommand {
    /// Show the virtual time: whether it's paused, its speed and the time elapsed.
    Status,
    /// Pause virtual time, freezing gameplay that uses it.
    Pause,
    /// Resume virtual time.
    Resume,
    /// Set how fast virtual time runs, relative to real time.
    Speed {
        /// The speed, such as 0.5 for half speed.
        speed: f64,
    },
}

/// Arguments of `hearth schedules`.
#[derive(Args, Debug)]
pub struct SchedulesArgs {
    /// The schedule whose systems to show.
    pub schedule: Option<String>,
}

/// Arguments of `hearth diagnostics`.
#[derive(Args, Debug)]
pub struct DiagnosticsArgs {
    /// Show only diagnostics whose path contains this text.
    pub pattern: Option<String>,
}

/// Arguments of `hearth call`.
#[derive(Args, Debug)]
pub struct CallArgs {
    /// The method, such as `world.query`.
    pub method: String,

    /// The parameters as JSON, `@file` or `-`.
    #[arg(allow_hyphen_values = true)]
    pub params: Option<String>,

    /// For `+watch` methods, when to stop.
    #[command(flatten)]
    pub stream: StreamArgs,
}

/// Arguments of `hearth exit`.
#[derive(Args, Debug)]
pub struct ExitArgs {
    /// Exit with this error code instead of success.
    #[arg(long, value_name = "CODE", value_parser = clap::value_parser!(u8).range(1..))]
    pub code: Option<u8>,

    /// Don't wait for the app to stop answering.
    #[arg(long)]
    pub no_wait: bool,
}

/// Arguments of `hearth commands`.
#[derive(Args, Debug)]
pub struct CommandsArgs {
    /// Show only commands whose name, description or arguments match this text (case-insensitive).
    #[arg(short, long, value_name = "TEXT")]
    pub grep: Option<String>,
}

/// Agent clients `hearth skill install` knows where to install for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum SkillClient {
    /// Claude Code: `~/.claude/skills`, or `.claude/skills` with `--project`.
    Claude,
    /// Codex: `~/.codex/skills`, or `.codex/skills` with `--project`.
    Codex,
    /// The shared `~/.agents/skills` directory, or `.agents/skills` with `--project`.
    Agents,
}

/// Where to install the skill.
#[derive(Args, Debug)]
pub struct SkillLocation {
    /// The agent client to install for.
    #[arg(value_enum, default_value_t = SkillClient::Claude)]
    pub client: SkillClient,

    /// Install into the current project instead of your home directory.
    #[arg(long, conflicts_with = "dir")]
    pub project: bool,

    /// Install into this skills directory instead.
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
}

/// The `hearth skill` commands.
#[derive(Subcommand, Debug)]
pub enum SkillCommand {
    /// Print the agent guide, or one of its reference topics.
    #[command(
        after_help = "Examples:\n  hearth skill show\n  hearth skill show values\n  hearth skill show --list"
    )]
    Show {
        /// The topic. Defaults to the main guide.
        topic: Option<String>,
        /// List the topics.
        #[arg(long)]
        list: bool,
    },
    /// Install the skill so agents find it. The installed file points back to `hearth skill show`,
    /// so it stays current as the CLI updates.
    #[command(
        after_help = "Examples:\n  hearth skill install\n  hearth skill install codex\n  hearth skill install --project\n  hearth skill install --full     # copy the whole guide instead of pointing to the CLI"
    )]
    Install {
        /// Where to install.
        #[command(flatten)]
        location: SkillLocation,
        /// Copy the full guide and references instead of a short file pointing to the CLI.
        #[arg(long)]
        full: bool,
        /// Replace an existing skill that this CLI didn't write.
        #[arg(long)]
        force: bool,
    },
    /// Remove the installed skill.
    Uninstall {
        /// Where it's installed.
        #[command(flatten)]
        location: SkillLocation,
    },
    /// Show whether and where the skill is installed.
    Status {
        /// Where to look.
        #[command(flatten)]
        location: SkillLocation,
    },
}

/// Arguments of `hearth mcp`.
#[derive(Args, Debug)]
pub struct McpArgs {
    /// Seconds a streaming tool (watch, observe, logs) runs when the call sets no limit.
    #[arg(long, value_name = "SECONDS", default_value_t = 5.0)]
    pub stream_duration: f64,

    /// Scale screenshots returned to the client so neither side exceeds this many pixels.
    #[arg(long, value_name = "PIXELS", default_value_t = 1280)]
    pub image_size: u32,
}
