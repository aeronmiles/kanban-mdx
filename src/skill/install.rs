//! Skill installation: writes embedded skill files to the target directory,
//! injecting a version comment into SKILL.md files.

use std::fs;
use std::path::Path;

use crate::skill::{embedded_files, inject_version_comment};

/// Installs the named skill to `target_dir/<skill_name>/`, injecting a version
/// comment. The target directory is the agent's skill base directory
/// (e.g. `.claude/skills/`).
pub fn install(skill_name: &str, target_dir: &Path, ver: &str) -> Result<(), String> {
    let files = embedded_files(skill_name);
    if files.is_empty() {
        return Err(format!("unknown skill: {skill_name}"));
    }

    let output_base = target_dir.join(skill_name);
    fs::create_dir_all(&output_base)
        .map_err(|e| format!("creating skill directory {}: {e}", output_base.display()))?;

    for file in &files {
        let dest_path = output_base.join(file.rel_path);

        // Ensure parent directory exists (for nested files like references/).
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("creating directory {}: {e}", parent.display()))?;
        }

        // Inject version comment into SKILL.md files.
        let content = if file.rel_path.ends_with("SKILL.md") {
            inject_version_comment(file.content, ver)
        } else {
            file.content.to_string()
        };

        fs::write(&dest_path, content)
            .map_err(|e| format!("writing {}: {e}", dest_path.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_kanban_md() {
        let tmp = tempfile::tempdir().unwrap();

        install("kanban-md", tmp.path(), "1.0.0").unwrap();

        // Check SKILL.md was written with version comment.
        let skill_md = tmp.path().join("kanban-md/SKILL.md");
        let data = fs::read_to_string(&skill_md).unwrap();
        assert!(data.contains("<!-- kanban-md-skill-version: 1.0.0 -->"));
        assert!(data.contains("kanban-md"));

        // Check references subdirectory.
        let ref_path = tmp.path().join("kanban-md/references/json-schemas.md");
        assert!(ref_path.exists());

        // Check version is readable.
        let ver = crate::skill::installed_version(&skill_md).unwrap();
        assert_eq!(ver, "1.0.0");
    }

    #[test]
    fn test_install_both_skills() {
        let tmp = tempfile::tempdir().unwrap();

        for s in crate::skill::AVAILABLE_SKILLS {
            install(s.name, tmp.path(), "2.0.0").unwrap();
        }

        let found = crate::skill::find_installed_skills(tmp.path());
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_install_overwrite() {
        let tmp = tempfile::tempdir().unwrap();

        install("kanban-md", tmp.path(), "1.0.0").unwrap();
        install("kanban-md", tmp.path(), "2.0.0").unwrap();

        let skill_md = tmp.path().join("kanban-md/SKILL.md");
        let ver = crate::skill::installed_version(&skill_md).unwrap();
        assert_eq!(ver, "2.0.0");
    }

    #[test]
    fn test_install_unknown_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let result = install("nonexistent", tmp.path(), "1.0.0");
        assert!(result.is_err());
    }
}
