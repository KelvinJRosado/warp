use std::path::{Path, PathBuf};

use ai::skills::{
    home_skills_path, read_skills, ParsedSkill, SkillProvider, SKILL_PROVIDER_DEFINITIONS,
};
use anyhow::Error;
use repo_metadata::{local_model::GetContentsArgs, RepoContent, RepoMetadataModel};
use warpui::AppContext;

use crate::warp_managed_paths_watcher::warp_managed_skill_dirs;

/// Finds all skill directories in a repository by querying the RepoMetadataModel tree.
///
/// Returns a list of paths to skill directories (e.g., `/repo/.agents/skills/`, `/repo/sub/.claude/skills/`).
pub fn find_skill_directories_in_tree(
    repo_path: &Path,
    repo_metadata: &RepoMetadataModel,
    ctx: &AppContext,
) -> Vec<PathBuf> {
    // Collect provider skills paths (e.g., ".agents/skills", ".claude/skills")
    let skill_path_suffixes: Vec<&Path> = SKILL_PROVIDER_DEFINITIONS
        .iter()
        .map(|p| p.skills_path.as_path())
        .collect();

    // Filter during traversal: only collect directories that end with a skill provider path.
    // The filter rejects files and non-matching directories, avoiding intermediate allocations.
    let args = GetContentsArgs::default().with_filter(move |content| {
        let RepoContent::Directory(dir) = content else {
            return false;
        };
        skill_path_suffixes
            .iter()
            .any(|suffix| dir.path.ends_with(&suffix.to_string_lossy()))
    });

    let Some(id) = repo_metadata::RepositoryIdentifier::try_local(repo_path) else {
        return Vec::new();
    };
    repo_metadata
        .get_repo_contents(&id, args, ctx)
        .unwrap_or_default()
        .into_iter()
        // Only directories should reach this iterator due to the GetContentsArgs::filter.
        // Keep the File arm for exhaustive matching in case RepoContent grows new variants.
        .map(|content| match content {
            RepoContent::Directory(dir) => dir.path.to_local_path_lossy(),
            RepoContent::File(f) => f.path.to_local_path_lossy(),
        })
        .collect()
}

/// Reads all skills from the given skill directories.
pub fn read_skills_from_directories(
    skill_dirs: impl IntoIterator<Item = PathBuf>,
) -> Vec<ParsedSkill> {
    skill_dirs
        .into_iter()
        .flat_map(|dir| read_skills(&dir))
        .collect()
}

pub fn is_skill_file(path: &Path) -> bool {
    extract_skill_parent_directory(path).is_ok()
}

pub fn extract_skill_parent_directory(path: &Path) -> Result<PathBuf, Error> {
    if path.file_name().and_then(|name| name.to_str()) != Some("SKILL.md") {
        return Err(anyhow::anyhow!("Not a skill path: {}", path.display()));
    }

    // Find the provider-specific skills directory. For example, if `path` is
    // `path/to/project/.claude/skills/my-skill/SKILL.md`, `provider_skills_dir`
    // is `path/to/project/.claude/skills`.
    let Some(provider_skills_dir) = path.parent().and_then(Path::parent) else {
        return Err(anyhow::anyhow!("Not a skill path: {}", path.display()));
    };

    if warp_managed_skill_dirs()
        .iter()
        .any(|dir| provider_skills_dir == dir)
    {
        return dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Home directory not available for {}", path.display()));
    }

    for provider in SKILL_PROVIDER_DEFINITIONS.iter() {
        if !provider_skills_dir.ends_with(&provider.skills_path) {
            continue;
        }

        let mut parent_directory = provider_skills_dir;
        for _ in provider.skills_path.components() {
            if let Some(parent) = parent_directory.parent() {
                parent_directory = parent;
            } else {
                return Err(anyhow::anyhow!("Not a skill path: {}", path.display()));
            }
        }

        if parent_directory.as_os_str().is_empty() {
            return Err(anyhow::anyhow!("Not a skill path: {}", path.display()));
        }

        return Ok(parent_directory.to_path_buf());
    }

    Err(anyhow::anyhow!("Not a skill path: {}", path.display()))
}

/// Check if this path is a skill directory under a home directory provider path
/// E.g. ~/.agents/skills/skill-name
pub fn is_home_skill_directory(path: &Path) -> bool {
    let parent_directory = path.parent();
    if let Some(parent_directory) = parent_directory {
        is_home_provider_path(parent_directory)
    } else {
        false
    }
}

/// Check if this path is a home directory provider path
/// E.g. ~/.agents/skills
pub fn is_home_provider_path(path: &Path) -> bool {
    SKILL_PROVIDER_DEFINITIONS.iter().any(|provider| {
        if provider.provider == SkillProvider::Warp {
            return warp_managed_skill_dirs().iter().any(|dir| path == dir);
        }
        home_skills_path(provider.provider)
            .as_ref()
            .is_some_and(|home_skills_path| path == home_skills_path)
    })
}

#[cfg(test)]
#[path = "utils_tests.rs"]
mod tests;
