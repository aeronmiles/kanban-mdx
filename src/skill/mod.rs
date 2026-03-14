//! Skill registry, agent discovery, installation, and version management.
//!
//! Skills are embedded markdown files that provide AI coding agents with
//! instructions for using kanban-mdx. This module handles discovering built-in
//! skills, installing them to agent-specific directories, and checking for
//! version staleness.

pub mod install;

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Embedded skill content
// ---------------------------------------------------------------------------

/// Embedded content of the kanban-mdx skill.
const KANBAN_MDX_SKILL: &str = include_str!("skills/kanban-mdx/SKILL.md");
/// Embedded content of the kanban-mdx JSON schemas reference.
const KANBAN_MDX_REFS_JSON_SCHEMAS: &str =
    include_str!("skills/kanban-mdx/references/json-schemas.md");
/// Embedded content of the kanban-based-development skill.
const KANBAN_BASED_DEV_SKILL: &str =
    include_str!("skills/kanban-based-development/SKILL.md");

// ---------------------------------------------------------------------------
// Skill info
// ---------------------------------------------------------------------------

/// Describes a built-in installable skill.
#[derive(Debug, Clone)]
pub struct SkillInfo {
    /// Directory name and identifier.
    pub name: &'static str,
    /// Short human-readable description.
    pub description: &'static str,
}

/// All available built-in skills.
pub const AVAILABLE_SKILLS: &[SkillInfo] = &[
    SkillInfo {
        name: "kanban-mdx",
        description: "Task management commands, workflows, and decision trees",
    },
    SkillInfo {
        name: "kanban-based-development",
        description: "Multi-agent parallel development workflow with claims and worktrees",
    },
];

/// Returns the names of all available skills.
pub fn skill_names() -> Vec<&'static str> {
    AVAILABLE_SKILLS.iter().map(|s| s.name).collect()
}

/// Reads the embedded SKILL.md content for the named skill.
pub fn read_embedded_skill(name: &str) -> Option<&'static str> {
    match name {
        "kanban-mdx" => Some(KANBAN_MDX_SKILL),
        "kanban-based-development" => Some(KANBAN_BASED_DEV_SKILL),
        _ => None,
    }
}

/// An embedded file to install for a skill.
#[derive(Debug)]
pub struct EmbeddedFile {
    /// Relative path within the skill directory.
    pub rel_path: &'static str,
    /// File content.
    pub content: &'static str,
}

/// Returns all embedded files for the named skill.
pub fn embedded_files(name: &str) -> Vec<EmbeddedFile> {
    match name {
        "kanban-mdx" => vec![
            EmbeddedFile {
                rel_path: "SKILL.md",
                content: KANBAN_MDX_SKILL,
            },
            EmbeddedFile {
                rel_path: "references/json-schemas.md",
                content: KANBAN_MDX_REFS_JSON_SCHEMAS,
            },
        ],
        "kanban-based-development" => vec![EmbeddedFile {
            rel_path: "SKILL.md",
            content: KANBAN_BASED_DEV_SKILL,
        }],
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Agent registry
// ---------------------------------------------------------------------------

/// Describes an AI coding agent and its skill directory conventions.
#[derive(Debug, Clone)]
pub struct Agent {
    /// Identifier used in --agent flags.
    pub name: &'static str,
    /// Human-readable name shown in output.
    pub display_name: &'static str,
    /// Skill directory relative to project root (empty for global-only agents).
    pub project_dir: &'static str,
    /// Skill directory relative to user home.
    pub global_dir: &'static str,
}

/// All supported AI coding agents.
const AGENTS: &[Agent] = &[
    Agent {
        name: "claude",
        display_name: "Claude Code",
        project_dir: ".claude/skills",
        global_dir: ".claude/skills",
    },
    Agent {
        name: "codex",
        display_name: "Codex",
        project_dir: ".agents/skills",
        global_dir: ".codex/skills",
    },
    Agent {
        name: "cursor",
        display_name: "Cursor",
        project_dir: ".cursor/skills",
        global_dir: ".cursor/skills",
    },
    Agent {
        name: "openclaw",
        display_name: "OpenClaw",
        project_dir: "", // global-only
        global_dir: ".openclaw/skills",
    },
];

/// Returns all supported agents.
pub fn agents() -> &'static [Agent] {
    AGENTS
}

/// Finds an agent by name, or None.
pub fn agent_by_name(name: &str) -> Option<&'static Agent> {
    AGENTS.iter().find(|a| a.name == name)
}

/// Returns the names of all supported agents.
pub fn all_agent_names() -> Vec<&'static str> {
    AGENTS.iter().map(|a| a.name).collect()
}

impl Agent {
    /// True if the agent only supports global skill installation.
    pub fn global_only(&self) -> bool {
        self.project_dir.is_empty()
    }

    /// Returns the absolute project-level skill path, or None for global-only agents.
    pub fn project_path(&self, project_root: &Path) -> Option<PathBuf> {
        if self.global_only() {
            return None;
        }
        Some(project_root.join(self.project_dir))
    }

    /// Returns the absolute global skill directory path.
    pub fn global_path(&self) -> Option<PathBuf> {
        home_dir().map(|home| home.join(self.global_dir))
    }

    /// Returns the appropriate install path: global for global-only agents,
    /// otherwise project path unless `global` is explicitly requested.
    pub fn skill_path(&self, project_root: &Path, global: bool) -> Option<PathBuf> {
        if global || self.global_only() {
            self.global_path()
        } else {
            self.project_path(project_root)
        }
    }
}

/// Detects agents whose project-level skill directory parent exists.
/// Global-only agents are never detected at the project level.
pub fn detect_agents(project_root: &Path) -> Vec<&'static Agent> {
    AGENTS
        .iter()
        .filter(|a| {
            if a.global_only() {
                return false;
            }
            // Check if parent directory of the skill dir exists.
            // e.g. for ".claude/skills", check if ".claude" exists.
            let parent = Path::new(a.project_dir)
                .parent()
                .unwrap_or(Path::new(""));
            let abs_parent = project_root.join(parent);
            abs_parent.is_dir()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Home directory helper
// ---------------------------------------------------------------------------

/// Returns the user's home directory using the HOME environment variable.
fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// Version management
// ---------------------------------------------------------------------------

const VERSION_PREFIX: &str = "<!-- kanban-mdx-skill-version: ";
const VERSION_SUFFIX: &str = " -->";

/// Returns the current CLI version for embedding in skills.
pub fn cli_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Formats the HTML comment line for embedding in SKILL.md files.
pub fn version_comment(ver: &str) -> String {
    format!("{VERSION_PREFIX}{ver}{VERSION_SUFFIX}")
}

/// Reads the version comment from an installed SKILL.md file.
/// Returns None if not found or the file doesn't exist.
pub fn installed_version(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    // Scan the first 30 lines for the version comment.
    for (i, line) in content.lines().enumerate() {
        if i >= 30 {
            break;
        }
        if let Some(ver) = extract_version_from_line(line) {
            return Some(ver);
        }
    }
    None
}

/// Extracts a version string from a single line if it matches the version comment pattern.
fn extract_version_from_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with(VERSION_PREFIX) && trimmed.ends_with(VERSION_SUFFIX) {
        let ver = &trimmed[VERSION_PREFIX.len()..trimmed.len() - VERSION_SUFFIX.len()];
        Some(ver.to_string())
    } else {
        None
    }
}

/// Returns true if the installed skill at `path` has a different version than
/// `current_version`. Returns false if the skill is not installed.
pub fn is_outdated(path: &Path, current_version: &str) -> bool {
    match installed_version(path) {
        Some(v) => v != current_version,
        None => false,
    }
}

/// Scans a base directory for installed kanban-mdx skills.
/// Returns a map of skill name to SKILL.md path.
pub fn find_installed_skills(base_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut result = Vec::new();
    for s in AVAILABLE_SKILLS {
        let skill_md = base_dir.join(s.name).join("SKILL.md");
        if installed_version(&skill_md).is_some() {
            result.push((s.name.to_string(), skill_md));
        }
    }
    result
}

/// Injects a version comment into skill content.
/// Inserts after the closing frontmatter delimiter (---), or prepends if no frontmatter.
pub fn inject_version_comment(content: &str, ver: &str) -> String {
    let comment = version_comment(ver);
    let lines: Vec<&str> = content.split('\n').collect();

    // Check for frontmatter: first line must be "---"
    if lines.len() < 2 || lines[0].trim() != "---" {
        // No frontmatter -- prepend the comment.
        return format!("{comment}\n{content}");
    }

    // Find the closing "---".
    for i in 1..lines.len() {
        if lines[i].trim() == "---" {
            // Insert comment after this line.
            let before: String = lines[..=i].join("\n");
            let after: String = lines[i + 1..].join("\n");
            return format!("{before}\n{comment}\n{after}");
        }
    }

    // No closing --- found -- prepend.
    format!("{comment}\n{content}")
}

/// Checks installed skills for the Claude agent and prints a staleness warning
/// to stderr if any are outdated. Called from PersistentPreRun equivalent.
pub fn check_skill_staleness(project_root: &Path) {
    let ver = cli_version();
    if ver == "dev" {
        return;
    }

    let claude = match agent_by_name("claude") {
        Some(a) => a,
        None => return,
    };

    let base_dir = match claude.project_path(project_root) {
        Some(p) => p,
        None => return,
    };

    let installed = find_installed_skills(&base_dir);
    for (_, skill_path) in &installed {
        if is_outdated(skill_path, ver) {
            if let Some(old_ver) = installed_version(skill_path) {
                eprintln!(
                    "hint: kanban-mdx skill outdated ({old_ver} -> {ver}), run: kbmdx skill update"
                );
                return; // One warning is enough.
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Find project root (for skill commands that don't need a board)
// ---------------------------------------------------------------------------

/// Finds the git root by walking up from the current directory.
/// Falls back to the current working directory if no .git is found.
pub fn find_project_root() -> std::io::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.clone();
    loop {
        if dir.join(".git").exists() {
            return Ok(dir);
        }
        match dir.parent() {
            Some(parent) => {
                if parent == dir {
                    return Ok(cwd);
                }
                dir = parent.to_path_buf();
            }
            None => return Ok(cwd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_available_skills() {
        let names = skill_names();
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "kanban-mdx");
        assert_eq!(names[1], "kanban-based-development");
    }

    #[test]
    fn test_read_embedded_skill() {
        let content = read_embedded_skill("kanban-mdx").unwrap();
        assert!(content.contains("kanban-mdx"));
        assert!(!content.is_empty());

        let content = read_embedded_skill("kanban-based-development").unwrap();
        assert!(content.contains("kanban-based-development") || content.contains("Kanban"));
        assert!(!content.is_empty());

        assert!(read_embedded_skill("nonexistent").is_none());
    }

    #[test]
    fn test_embedded_files() {
        let files = embedded_files("kanban-mdx");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].rel_path, "SKILL.md");
        assert_eq!(files[1].rel_path, "references/json-schemas.md");

        let files = embedded_files("kanban-based-development");
        assert_eq!(files.len(), 1);

        let files = embedded_files("nonexistent");
        assert!(files.is_empty());
    }

    #[test]
    fn test_agent_registry() {
        let all = agents();
        assert_eq!(all.len(), 4);

        let names = all_agent_names();
        assert_eq!(names, vec!["claude", "codex", "cursor", "openclaw"]);
    }

    #[test]
    fn test_agent_by_name() {
        let a = agent_by_name("claude").unwrap();
        assert_eq!(a.display_name, "Claude Code");
        assert_eq!(a.project_dir, ".claude/skills");

        assert!(agent_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_global_only_agent() {
        let oc = agent_by_name("openclaw").unwrap();
        assert!(oc.global_only());
        assert!(oc.project_path(Path::new("/some/root")).is_none());
        assert!(oc.global_path().is_some());

        // SkillPath should return global path even without --global flag.
        let sp = oc.skill_path(Path::new("/some/root"), false);
        assert_eq!(sp, oc.global_path());

        let cl = agent_by_name("claude").unwrap();
        assert!(!cl.global_only());
    }

    #[test]
    fn test_detect_agents() {
        let tmp = tempfile::tempdir().unwrap();
        // Create .claude/ directory.
        fs::create_dir_all(tmp.path().join(".claude")).unwrap();

        let detected = detect_agents(tmp.path());
        let names: Vec<&str> = detected.iter().map(|a| a.name).collect();
        assert!(names.contains(&"claude"));
        assert!(!names.contains(&"openclaw"));
        assert!(!names.contains(&"codex"));
    }

    #[test]
    fn test_version_comment() {
        let comment = version_comment("1.2.3");
        assert_eq!(comment, "<!-- kanban-mdx-skill-version: 1.2.3 -->");
    }

    #[test]
    fn test_installed_version() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("SKILL.md");

        let content = "---\nname: test\n---\n<!-- kanban-mdx-skill-version: 0.19.0 -->\n# Test\n";
        fs::write(&path, content).unwrap();

        let ver = installed_version(&path).unwrap();
        assert_eq!(ver, "0.19.0");
    }

    #[test]
    fn test_installed_version_missing() {
        let ver = installed_version(Path::new("/nonexistent/path/SKILL.md"));
        assert!(ver.is_none());
    }

    #[test]
    fn test_installed_version_no_comment() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("SKILL.md");

        let content = "---\nname: test\n---\n# Test\nNo version here.\n";
        fs::write(&path, content).unwrap();

        assert!(installed_version(&path).is_none());
    }

    #[test]
    fn test_is_outdated() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("SKILL.md");

        let content = "---\nname: test\n---\n<!-- kanban-mdx-skill-version: 0.18.0 -->\n# Test\n";
        fs::write(&path, content).unwrap();

        assert!(is_outdated(&path, "0.19.0"));
        assert!(!is_outdated(&path, "0.18.0"));
    }

    #[test]
    fn test_inject_version_comment_with_frontmatter() {
        let input = "---\nname: test\ndescription: a test\n---\n# Title\nBody\n";
        let result = inject_version_comment(input, "1.0.0");

        assert!(result.contains("---\n<!-- kanban-mdx-skill-version: 1.0.0 -->"));
        assert!(result.starts_with("---\n"));
    }

    #[test]
    fn test_inject_version_comment_without_frontmatter() {
        let input = "# Title\nNo frontmatter\n";
        let result = inject_version_comment(input, "1.0.0");

        assert!(result.starts_with("<!-- kanban-mdx-skill-version: 1.0.0 -->\n"));
    }

    #[test]
    fn test_find_installed_skills() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a skill with a version comment.
        let skill_dir = tmp.path().join("kanban-mdx");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: test\n---\n<!-- kanban-mdx-skill-version: 1.0.0 -->\n# Test\n",
        )
        .unwrap();

        let found = find_installed_skills(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "kanban-mdx");
    }
}
