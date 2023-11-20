#![warn(clippy::pedantic)]

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::{fs, process::Command};

#[derive(clap::Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand)]
pub enum Commands {
    Install,
    Update,
    Clean,
}

const DEPENDENCIES_PATH: &str = "tests/dependencies/";

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub name: String,
    pub dependencies: Dependencies,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Dependencies {
    pub rolling: Vec<String>,
    pub commit: Vec<CommitDependency>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct CommitDependency {
    pub repository: String,
    pub commit: String,
}

/// Clone rolling dependencies in ``<repository_name>.rolling`` directories.
/// Clone commit dependencies in ``<repository_name>.commit`` directories and
/// checkout to the respective commits. Clone only those repositories,
/// that are not in the lock file.
pub fn install(config_dependencies: &Dependencies, locked_dependencies: &Dependencies) {
    for config_rolling_dependency in &config_dependencies.rolling {
        if !locked_dependencies
            .rolling
            .contains(config_rolling_dependency)
        {
            let _ = Command::new("git")
                .current_dir(DEPENDENCIES_PATH)
                .arg("clone")
                .arg(config_rolling_dependency)
                .arg(repository_name(config_rolling_dependency).to_string() + ".rolling")
                .output();
        }
    }
    for config_commit_dependency in &config_dependencies.commit {
        if !locked_dependencies
            .commit
            .contains(config_commit_dependency)
        {
            let dir = repository_name(&config_commit_dependency.repository).to_string() + ".commit";
            let _ = Command::new("git")
                .current_dir(DEPENDENCIES_PATH)
                .arg("clone")
                .arg(&config_commit_dependency.repository)
                .arg(&dir)
                .output();
            let _ = Command::new("git")
                .current_dir(DEPENDENCIES_PATH.to_string() + &dir)
                .arg("checkout")
                .arg(&config_commit_dependency.commit)
                .output();
        }
    }
    lock(config_dependencies);
}

/// Rolling dependencies are only updated.
pub fn update(locked_rolling_dependencies: &[&str]) {
    for dependency in locked_rolling_dependencies {
        let _ = Command::new("git")
            .current_dir(DEPENDENCIES_PATH.to_string() + repository_name(dependency) + ".rolling")
            .arg("pull")
            .output();
    }
}

/// Delete directories, that are not in config file.
///
/// # Panics
///
/// Impossible
pub fn clean(config_dependencies: &Dependencies) {
    let repository_names: Vec<&String> = config_dependencies
        .rolling
        .iter()
        .chain(config_dependencies.commit.iter().map(|x| &x.repository))
        .collect();
    for file in fs::read_dir(DEPENDENCIES_PATH).expect("The directory for project should exist") {
        let dir_name = file.unwrap().file_name().to_str().unwrap().to_string();
        if !repository_names
            .iter()
            .any(|x| Regex::new(repository_name(x)).unwrap().is_match(&dir_name))
        {
            let _ = fs::remove_dir_all(dir_name);
        }
    }
}

/// ``git_url`` must consist ``.git`` part
fn repository_name(git_url: &str) -> &str {
    Regex::new(r"/(\w*)\.git")
        .expect(r"'/(\w*)\.git' should be valid")
        .captures(git_url)
        .expect(r"'/(\w*)\.git' should be right to extract a repository name")
        .get(1)
        .expect(r"'/(\w*)\.git' should be right to extract a repository name")
        .as_str()
}

fn lock(dependencies: &Dependencies) {
    let _ = fs::write(
        DEPENDENCIES_PATH.to_string() + "crack.lock",
        toml::to_string(dependencies).expect("fs::write should fall only in very weird situations"),
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
