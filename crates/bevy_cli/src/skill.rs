//! The agent skill: a guide that teaches agents to use this CLI. `hearth skill show` prints it and
//! `hearth skill install` puts it where agent clients look for skills.
//!
//! By default the installed skill is a short file that points back to `hearth skill show`, so it
//! never goes stale as the CLI updates. `--full` copies the whole guide instead, for clients that
//! can't run commands while loading skills.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::cli::{SkillClient, SkillLocation};
use crate::context::Ctx;
use crate::output::{table, CliError, Exit, Outcome};

/// The guide, with the frontmatter skill loaders read.
pub const GUIDE: &str = include_str!("../skill/SKILL.md");

/// Reference topics: name, what it covers, text.
pub const TOPICS: &[(&str, &str, &str)] = &[
    (
        "values",
        "Writing values: vectors, quaternions, enums, options, entities, colors and field paths",
        include_str!("../skill/references/values.md"),
    ),
    (
        "recipes",
        "Step-by-step recipes: verify a change, drive input, wait for events, debug a crash",
        include_str!("../skill/references/recipes.md"),
    ),
    (
        "errors",
        "Error codes and exit codes, and how to recover from each",
        include_str!("../skill/references/errors.md"),
    ),
    (
        "brp",
        "The Bevy Remote Protocol behind the commands, for `hearth call`",
        include_str!("../skill/references/brp.md"),
    ),
];

/// The name of the skill's folder.
const NAME: &str = "hearth";

/// The comment that marks a `SKILL.md` this CLI wrote.
const MARKER: &str = "<!-- installed by hearth-cli";

/// The guide without its frontmatter.
fn body(text: &str) -> &str {
    text.strip_prefix("---")
        .and_then(|rest| rest.split_once("\n---"))
        .map(|(_, body)| body.trim_start_matches(['\r', '\n']))
        .unwrap_or(text)
}

/// The skill's description, from the guide's frontmatter.
fn description() -> &'static str {
    GUIDE
        .lines()
        .find_map(|line| line.strip_prefix("description:"))
        .map(str::trim)
        .unwrap_or_default()
}

/// The short skill that points to `hearth skill show`.
fn pointer() -> String {
    format!(
        "---\nname: {NAME}\ndescription: {}\n---\n\n# Hearth CLI\n\nThe `hearth` command line tool builds, runs, inspects and drives Hearth apps. Its guide ships\nwith the tool, so it always matches the installed version. Read it before using `bevy`:\n\n```bash\nbevy skill show           # the guide: workflow, output, errors\nbevy skill show --list    # reference topics, such as values and recipes\n```\n\n{MARKER} {} (pointer) -->\n",
        description(),
        env!("CARGO_PKG_VERSION")
    )
}

fn home_dir() -> Result<PathBuf, CliError> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            CliError::new(
                "IO_ERROR",
                Exit::Error,
                format!("Can't find your home directory: {var} isn't set"),
            )
            .hint("Pass --dir with the skills directory to use.")
        })
}

/// The root of the project the current directory is in: the nearest folder with `.git`, else
/// the nearest with `Cargo.toml`, else the current directory.
fn project_root() -> Result<PathBuf, CliError> {
    let cwd =
        std::env::current_dir().map_err(|error| CliError::io("the current directory", error))?;
    let find = |marker: &str| {
        cwd.ancestors()
            .find(|dir| dir.join(marker).exists())
            .map(Path::to_path_buf)
    };
    Ok(find(".git").or_else(|| find("Cargo.toml")).unwrap_or(cwd))
}

/// The skills directory of a client, in the home directory or in the project.
fn skills_root(client: SkillClient, project: bool) -> Result<PathBuf, CliError> {
    let env_home = |var: &str| {
        std::env::var_os(var)
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
    };
    if project {
        let root = project_root()?;
        return Ok(match client {
            SkillClient::Claude => root.join(".claude").join("skills"),
            SkillClient::Codex => root.join(".codex").join("skills"),
            SkillClient::Agents => root.join(".agents").join("skills"),
        });
    }
    Ok(match client {
        SkillClient::Claude => env_home("CLAUDE_CONFIG_DIR")
            .map_or_else(|| home_dir().map(|home| home.join(".claude")), Ok)?
            .join("skills"),
        SkillClient::Codex => env_home("CODEX_HOME")
            .map_or_else(|| home_dir().map(|home| home.join(".codex")), Ok)?
            .join("skills"),
        SkillClient::Agents => home_dir()?.join(".agents").join("skills"),
    })
}

/// The folder the skill goes in.
fn skill_dir(location: &SkillLocation) -> Result<PathBuf, CliError> {
    let root = match &location.dir {
        Some(dir) => dir.clone(),
        None => skills_root(location.client, location.project)?,
    };
    Ok(root.join(NAME))
}

/// Where the skill is installed for `client` in the home directory, if it is.
pub fn installed(client: SkillClient) -> Option<PathBuf> {
    let file = skills_root(client, false).ok()?.join(NAME).join("SKILL.md");
    file.is_file().then_some(file)
}

/// What an installed `SKILL.md` is: `pointer` or `full` with the CLI version that wrote it, or
/// `foreign` when this CLI didn't write it.
fn describe(text: &str) -> (&'static str, Option<String>) {
    let Some(start) = text.find(MARKER) else {
        return ("foreign", None);
    };
    let rest = &text[start + MARKER.len()..];
    let rest = rest.split("-->").next().unwrap_or_default();
    let version = rest.split_whitespace().next().map(str::to_string);
    let mode = if rest.contains("(full)") {
        "full"
    } else {
        "pointer"
    };
    (mode, version)
}

fn client_name(client: SkillClient) -> &'static str {
    match client {
        SkillClient::Claude => "claude",
        SkillClient::Codex => "codex",
        SkillClient::Agents => "agents",
    }
}

/// `hearth skill show`.
pub fn show(ctx: &mut Ctx, topic: Option<&str>, list: bool) -> Result<Outcome, CliError> {
    if list {
        let rows: Vec<Vec<String>> = TOPICS
            .iter()
            .map(|(name, about, _)| vec![(*name).to_string(), (*about).to_string()])
            .collect();
        return Ok(Outcome::new(
            json!(TOPICS
                .iter()
                .map(|(name, about, _)| json!({ "topic": name, "about": about }))
                .collect::<Vec<_>>()),
            format!(
                "{}Show one with `hearth skill show <topic>`.",
                table(ctx.out.color, &["TOPIC", "ABOUT"], &rows)
            ),
        ));
    }
    let Some(topic) = topic else {
        let text = body(GUIDE);
        return Ok(Outcome::new(json!({ "topic": null, "text": text }), text));
    };
    let lower = topic.to_lowercase();
    match TOPICS.iter().find(|(name, _, _)| *name == lower) {
        Some((name, _, text)) => Ok(Outcome::new(json!({ "topic": name, "text": text }), *text)),
        None => Err(CliError::new(
            "UNKNOWN_TOPIC",
            Exit::Usage,
            format!("The skill has no topic named `{topic}`"),
        )
        .hint("`hearth skill show --list` lists the topics.")
        .data(
            json!({ "candidates": TOPICS.iter().map(|(name, _, _)| *name).collect::<Vec<_>>() }),
        )),
    }
}

/// `hearth skill install`.
pub fn install(location: &SkillLocation, full: bool, force: bool) -> Result<Outcome, CliError> {
    let dir = skill_dir(location)?;
    let file = dir.join("SKILL.md");
    if let Ok(existing) = std::fs::read_to_string(&file)
        && describe(&existing).0 == "foreign"
        && !force
    {
        return Err(CliError::new(
            "SKILL_EXISTS",
            Exit::Rejected,
            format!(
                "{} already has a skill this CLI didn't write",
                crate::commands::display_path(&file)
            ),
        )
        .hint("Pass --force to replace it."));
    }
    std::fs::create_dir_all(&dir).map_err(|error| CliError::io(dir.display(), error))?;
    let version = env!("CARGO_PKG_VERSION");
    let text = if full {
        format!("{}\n\n{MARKER} {version} (full) -->\n", GUIDE.trim_end())
    } else {
        pointer()
    };
    std::fs::write(&file, text).map_err(|error| CliError::io(file.display(), error))?;
    let references = dir.join("references");
    let mut files = vec![file.clone()];
    if full {
        std::fs::create_dir_all(&references)
            .map_err(|error| CliError::io(references.display(), error))?;
        for (name, _, text) in TOPICS {
            let path = references.join(format!("{name}.md"));
            std::fs::write(&path, text).map_err(|error| CliError::io(path.display(), error))?;
            files.push(path);
        }
    } else {
        remove_references(&references);
    }
    let shown = crate::commands::display_path(&file);
    Ok(Outcome::new(
        json!({
            "client": client_name(location.client),
            "path": shown,
            "mode": if full { "full" } else { "pointer" },
            "version": version,
            "files": files.iter().map(|path| crate::commands::display_path(path)).collect::<Vec<_>>(),
        }),
        format!(
            "Installed the {} skill at {shown}{}",
            if full { "full" } else { "hearth" },
            if full {
                ""
            } else {
                "\nIt points agents to `hearth skill show`, so it stays current as the CLI updates."
            }
        ),
    ))
}

/// Removes the reference files a full install wrote, and their folder when it's left empty.
fn remove_references(references: &Path) {
    for (name, _, _) in TOPICS {
        let _ = std::fs::remove_file(references.join(format!("{name}.md")));
    }
    let _ = std::fs::remove_dir(references);
}

/// `hearth skill uninstall`.
pub fn uninstall(location: &SkillLocation) -> Result<Outcome, CliError> {
    let dir = skill_dir(location)?;
    let file = dir.join("SKILL.md");
    let shown = crate::commands::display_path(&file);
    let Ok(existing) = std::fs::read_to_string(&file) else {
        return Ok(Outcome::new(
            json!({ "path": shown, "removed": false }),
            format!("No skill is installed at {shown}"),
        ));
    };
    if describe(&existing).0 == "foreign" {
        return Err(CliError::new(
            "SKILL_EXISTS",
            Exit::Rejected,
            format!("{shown} wasn't written by this CLI, so it was left alone"),
        ));
    }
    std::fs::remove_file(&file).map_err(|error| CliError::io(file.display(), error))?;
    remove_references(&dir.join("references"));
    let _ = std::fs::remove_dir(&dir);
    Ok(Outcome::new(
        json!({ "path": shown, "removed": true }),
        format!("Removed the skill at {shown}"),
    ))
}

/// `hearth skill status`.
pub fn status(location: &SkillLocation) -> Result<Outcome, CliError> {
    let dir = skill_dir(location)?;
    let file = dir.join("SKILL.md");
    let shown = crate::commands::display_path(&file);
    let Ok(existing) = std::fs::read_to_string(&file) else {
        return Ok(Outcome::new(
            json!({ "client": client_name(location.client), "path": shown, "installed": false }),
            format!("Not installed (it would go to {shown}). `hearth skill install` installs it."),
        ));
    };
    let (mode, version) = describe(&existing);
    let current = env!("CARGO_PKG_VERSION");
    let outdated = mode == "full" && version.as_deref() != Some(current);
    let human = match mode {
        "foreign" => format!("{shown} holds a skill this CLI didn't write"),
        "full" if outdated => format!(
            "Installed at {shown}: a full copy from version {}, older than this CLI ({current}). Run `hearth skill install --full` to update it.",
            version.as_deref().unwrap_or("?")
        ),
        "full" => format!("Installed at {shown}: a full copy of the guide"),
        _ => format!("Installed at {shown}: it points to `hearth skill show`, so it stays current"),
    };
    Ok(Outcome::new(
        json!({
            "client": client_name(location.client),
            "path": shown,
            "installed": true,
            "mode": mode,
            "version": version,
            "outdated": outdated,
        }),
        human,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_has_frontmatter() {
        // A Windows checkout may have CRLF line endings.
        assert!(GUIDE
            .replace("\r\n", "\n")
            .starts_with("---\nname: hearth\n"));
        assert!(!description().is_empty());
        assert!(!body(GUIDE).starts_with("---"));
        assert!(body(GUIDE).starts_with('#'));
    }

    #[test]
    fn errors_topic_covers_every_code() {
        let errors = TOPICS
            .iter()
            .find(|(name, _, _)| *name == "errors")
            .unwrap()
            .2;
        for (code, _) in crate::commands::ERROR_CODES {
            assert!(
                errors.contains(&format!("`{code}`")),
                "the errors topic doesn't explain {code}"
            );
        }
    }

    #[test]
    fn recognizes_installed_skills() {
        assert_eq!(
            describe(&pointer()),
            ("pointer", Some(env!("CARGO_PKG_VERSION").to_string()))
        );
        assert_eq!(
            describe("<!-- installed by hearth-cli 0.1.0 (full) -->"),
            ("full", Some("0.1.0".to_string()))
        );
        assert_eq!(describe("---\nname: hearth\n---\nmine"), ("foreign", None));
    }

    #[test]
    fn installs_and_uninstalls() {
        let dir =
            std::env::temp_dir().join(format!("hearth-cli-skill-test-{}", std::process::id()));
        let location = SkillLocation {
            client: SkillClient::Claude,
            project: false,
            dir: Some(dir.clone()),
        };
        install(&location, true, false).unwrap();
        assert!(dir.join("hearth/references/values.md").is_file());
        assert_eq!(status(&location).unwrap().data["mode"], "full");
        install(&location, false, false).unwrap();
        assert!(!dir.join("hearth/references").exists());
        assert_eq!(status(&location).unwrap().data["mode"], "pointer");
        std::fs::write(dir.join("hearth/SKILL.md"), "mine").unwrap();
        assert_eq!(
            install(&location, false, false).unwrap_err().code,
            "SKILL_EXISTS"
        );
        assert_eq!(uninstall(&location).unwrap_err().code, "SKILL_EXISTS");
        install(&location, false, true).unwrap();
        assert_eq!(uninstall(&location).unwrap().data["removed"], true);
        assert!(!dir.join("hearth").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
