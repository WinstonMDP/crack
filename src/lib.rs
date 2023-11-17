#![warn(clippy::pedantic)]

use std::process::Command;

const DEPENDENCIES_PATH: &str = "tests/dependencies/";

/// Clone rolling dependencies in ``<repository_name>.rolling`` directories.
pub fn clone_rolling_dependencies(repositories: &[&str]) {
    for repository in repositories {
        let _ = Command::new("git")
            .current_dir(DEPENDENCIES_PATH)
            .arg("clone")
            .arg(repository)
            .arg(repository_name(repository).to_string() + ".rolling")
            .output();
    }
}

#[derive(serde::Serialize)]
pub struct CommitDependency {
    pub repository: String,
    pub commit: String,
}

/// Clone commit dependencies in ``<repository_name>.commit`` directories and
/// checkout to the respective commits.
pub fn clone_commit_dependencies(commit_dependencies: &[CommitDependency]) {
    for commit_dependency in commit_dependencies {
        let dir = repository_name(&commit_dependency.repository).to_string() + ".commit";
        let _ = Command::new("git")
            .current_dir(DEPENDENCIES_PATH)
            .arg("clone")
            .arg(&commit_dependency.repository)
            .arg(&dir)
            .output();
        let _ = Command::new("git")
            .current_dir(DEPENDENCIES_PATH.to_string() + &dir)
            .arg("checkout")
            .arg(&commit_dependency.commit)
            .output();
    }
    lock(commit_dependencies);
}

/// ``git_url`` must consist ``.git`` part
fn repository_name(git_url: &str) -> &str {
    regex::Regex::new(r"/(\w*)\.git")
        .expect(r"'/(\w*)\.git' should be valid")
        .captures(git_url)
        .expect(r"'/(\w*)\.git' should be right to extract a repository name")
        .get(1)
        .expect(r"'/(\w*)\.git' should be right to extract a repository name")
        .as_str()
}

#[derive(serde::Serialize)]
struct CommitDependencies<'a> {
    commit_dependencies: &'a [CommitDependency],
}

fn lock(commit_dependencies: &[CommitDependency]) {
    let _ = std::fs::write(
        DEPENDENCIES_PATH.to_string() + "crack.lock",
        toml::to_string(&CommitDependencies {
            commit_dependencies,
        })
        .expect("fs::write should fall only in very weird situations"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_name_t_1() {
        assert_eq!(
            repository_name("https://github.com/WinstonMDP/repo_name.git"),
            "repo_name"
        );
    }

    #[test]
    fn repository_name_t_2() {
        assert_eq!(
            repository_name("ssh://[user@]host.xz[:port]/~[user]/path/to/repo.git/"),
            "repo"
        );
    }
}
