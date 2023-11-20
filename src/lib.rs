#![warn(clippy::pedantic)]

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::{fs, process::Command};

#[derive(clap::Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Subcommand,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Install crak.toml dependencies, that are not in the crack.lock
    I,
    /// Update crack.lock dependencies
    U,
    /// Delete directories, that are not in the crack.toml
    C,
}

const DEPENDENCIES_PATH: &str = "target/dependencies/";

#[derive(Deserialize, Serialize, Debug)]
pub struct Cfg {
    pub name: String,
    pub dependencies: Dependencies,
}

impl Cfg {
    fn new() -> Self {
        toml::from_str(&fs::read_to_string("crack.toml").expect("crack.toml should exist"))
            .expect("crack.toml should be valid for deserialisation")
    }
}

#[derive(Deserialize, Serialize, Debug, Default)]
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
pub fn install() {
    let cfg = Cfg::new();
    let locked_dependencies = locked_dependencies();
    for cfg_rolling_dependency in &cfg.dependencies.rolling {
        if !locked_dependencies.rolling.contains(cfg_rolling_dependency) {
            let _ = Command::new("git")
                .current_dir(DEPENDENCIES_PATH)
                .arg("clone")
                .arg(cfg_rolling_dependency)
                .arg(repository_name(cfg_rolling_dependency).to_string() + ".rolling")
                .output();
        }
    }
    for cfg_commit_dependency in &cfg.dependencies.commit {
        if !locked_dependencies.commit.contains(cfg_commit_dependency) {
            let dir = repository_name(&cfg_commit_dependency.repository).to_string()
                + "."
                + &cfg_commit_dependency.commit
                + ".commit";
            let _ = Command::new("git")
                .current_dir(DEPENDENCIES_PATH)
                .arg("clone")
                .arg(&cfg_commit_dependency.repository)
                .arg(&dir)
                .output();
            let _ = Command::new("git")
                .current_dir(DEPENDENCIES_PATH.to_string() + &dir)
                .arg("checkout")
                .arg(&cfg_commit_dependency.commit)
                .output();
        }
    }
    lock(&cfg.dependencies);
}

/// Rolling dependencies are only updated.
pub fn update() {
    let locked_dependencies = locked_dependencies();
    for dependency in locked_dependencies.rolling {
        let _ = Command::new("git")
            .current_dir(DEPENDENCIES_PATH.to_string() + repository_name(&dependency) + ".rolling")
            .arg("pull")
            .output();
    }
}

/// Delete directories, that are not in cfg file.
#[allow(clippy::missing_panics_doc)]
pub fn clean() {
    let cfg = Cfg::new();
    let repository_names: Vec<&String> = cfg
        .dependencies
        .rolling
        .iter()
        .chain(cfg.dependencies.commit.iter().map(|x| &x.repository))
        .collect();
    for file in fs::read_dir(DEPENDENCIES_PATH).expect("the directory for project should exist") {
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
// '-' character must be included to valid

fn lock(dependencies: &Dependencies) {
    let _ = fs::write(
        "crack.lock",
        toml::to_string(dependencies).expect("fs::write should fall only in very weird situations"),
    );
}

fn locked_dependencies() -> Dependencies {
    toml::from_str(&fs::read_to_string("crack.lock").unwrap_or_default()) // to fix
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn repository_name_t_1() {
        assert_eq!(
            super::repository_name("https://github.com/WinstonMDP/repo_name.git"),
            "repo_name"
        );
    }

    #[test]
    fn repository_name_t_2() {
        assert_eq!(
            super::repository_name("ssh://[user@]host.xz[:port]/~[user]/path/to/repo.git/"),
            "repo"
        );
    }
}
