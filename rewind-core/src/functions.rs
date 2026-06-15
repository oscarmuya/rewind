use std::path::{Path, PathBuf};

/// Resolves the git repo root and current branch for a given directory.
/// Returns (None, None) if the path is not inside a git repo.
pub fn resolve_git(cwd: &str) -> (Option<String>, Option<String>) {
    let repo = match git2::Repository::discover(Path::new(cwd)) {
        Ok(repo) => repo,
        Err(_) => return (None, None),
    };

    let repo_root = repo
        .workdir()
        .or_else(|| repo.path().parent())
        .and_then(Path::to_str)
        .map(|s| s.trim_end_matches('/').to_owned());

    let branch = match repo.head() {
        Ok(head) => head.shorthand().ok().map(|s| s.to_owned()),
        Err(_) => None,
    };

    (repo_root, branch)
}

/// Walks up the directory tree from `cwd` looking for a `.git` entry.
/// Returns the first directory containing `.git` (the project root), or
/// falls back to `cwd` itself if no git repository is found.
pub fn find_project_root(cwd: &Path) -> PathBuf {
    let mut dir = cwd;
    loop {
        if dir.join(".git").exists() {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            Option::None => return cwd.to_path_buf(),
        }
    }
}
