// TODO: error handling
// TODO: progress bar
// TODO: more efficient git calls

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

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

/// ``crack.toml``
#[derive(Deserialize, Serialize, Debug)]
pub struct Cfg {
    pub name: String,
    #[serde(default)]
    pub dependencies: Dependencies,
}

impl Cfg {
    fn new(cfg_dir: &Path) -> Self {
        // TODO: must be Result
        toml::from_str(
            &fs::read_to_string(cfg_dir.join("crack.toml")).expect("crack.toml should exist"),
        )
        .expect("crack.toml should be valid for deserialisation")
    }
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct Dependencies {
    #[serde(default)]
    pub rolling: Vec<RollingDependency>,
    #[serde(default)]
    pub commit: Vec<CommitDependency>,
}

impl Dependencies {
    fn append(&mut self, mut other: Self) {
        self.rolling.append(&mut other.rolling);
        self.commit.append(&mut other.commit);
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct RollingDependency {
    pub repo: String,
    pub branch: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct CommitDependency {
    pub repo: String,
    pub commit: String,
}

/// Clone rolling dependencies in ``<repo_name>.<branch>.rolling`` directories
/// and checkout to the respective branch.
/// Clone commit dependencies in ``<repo_name>.<commit>.commit`` directories and
/// checkout to the respective commits.
/// Clone only those repositories, that are not in the directory.
#[must_use]
pub fn install(cfg_dir: &Path, dependencies_dir: &Path) -> Dependencies {
    // TODO: must be Result
    let cfg = Cfg::new(cfg_dir);
    let mut dependencies_to_lock = Dependencies::default();
    for dependency in &cfg.dependencies.rolling {
        let dir = rolling_dependency_dir(dependency);
        if !Path::new(&dir).exists() {
            let mut command = Command::new("git");
            let mut command = command
                .current_dir(dependencies_dir)
                .arg("clone")
                .arg(&dependency.repo);
            if let Some(branch) = dependency.branch.clone() {
                command = command.arg("-b").arg(branch);
            }
            let _ = command.arg(&dir).output();
            dependencies_to_lock.append(install(&dependencies_dir.join(&dir), dependencies_dir));
        }
    }
    for dependency in &cfg.dependencies.commit {
        let dir_name =
            repo_name(&dependency.repo).to_string() + "." + &dependency.commit + ".commit";
        if !Path::new(&dir_name).exists() {
            let _ = Command::new("git")
                .current_dir(dependencies_dir)
                .arg("clone")
                .arg(&dependency.repo)
                .arg(&dir_name)
                .output();
            let dir = dependencies_dir.join(&dir_name);
            let _ = Command::new("git")
                .current_dir(&dir)
                .arg("checkout")
                .arg(&dependency.commit)
                .output();
            dependencies_to_lock.append(install(&dir, dependencies_dir));
        }
    }
    dependencies_to_lock
}

/// Rolling dependencies are only updated.
pub fn update(locked_dependencies: &Dependencies, dependencies_dir: &Path) {
    // TODO: must be Result
    for dependency in &locked_dependencies.rolling {
        let _ = Command::new("git")
            .current_dir(dependencies_dir.join(rolling_dependency_dir(dependency)))
            .arg("pull")
            .output();
    }
}

/// Delete directories, that are not in crack.toml.
pub fn clean(locked_dependencies: &Dependencies, dependencies_dir: &Path) {
    // TODO: must be Result
    for file in fs::read_dir(dependencies_dir).expect("the directory for project should exist") {
        let dir_name = file.unwrap().file_name().to_str().unwrap().to_string(); // TODO: fix those calls
        if !locked_dependencies
            .rolling
            .iter()
            .any(|x| Regex::new(repo_name(&x.repo)).unwrap().is_match(&dir_name))
            && !locked_dependencies.commit.iter().any(|x| {
                Regex::new(&(repo_name(&x.repo).to_string() + "." + &x.commit))
                    .unwrap()
                    .is_match(&dir_name)
            })
        {
            let _ = fs::remove_dir_all(dependencies_dir.join(&dir_name));
        }
    }
}

fn rolling_dependency_dir(dependency: &RollingDependency) -> String {
    repo_name(&dependency.repo).to_string()
        + "."
        + &dependency.branch.clone().unwrap_or("default".to_string())
        + ".rolling"
}

/// ``git_url`` must consist ``.git`` part
fn repo_name(git_url: &str) -> &str {
    Regex::new(r"/([^/]*)\.git")
        .expect("the regex should be valid")
        .captures(git_url)
        .expect("the regex should be right to extract a repository name")
        .get(1)
        .expect("the regex should be right to extract a repository name")
        .as_str()
}

#[must_use]
pub fn project_root() -> Option<PathBuf> {
    let current_dir = std::env::current_dir().unwrap();
    for i in current_dir.ancestors() {
        if i.join("crack.toml").exists() {
            return Some(i.to_path_buf());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    #[test]
    fn repo_name_t_1() {
        assert_eq!(
            super::repo_name("https://github.com/WinstonMDP/repo_name.git"),
            "repo_name"
        );
    }

    #[test]
    fn repo_name_t_2() {
        assert_eq!(
            super::repo_name("ssh://[user@]host.xz[:port]/~[user]/path/to/repo.git/"),
            "repo"
        );
    }

    #[test]
    fn repo_name_t_3() {
        assert_eq!(
            super::repo_name("https://github.com/WinstonMDP/repo-name.git"),
            "repo-name"
        );
    }

    fn nfiles(dir: &Path) -> usize {
        dir.read_dir().unwrap().collect::<Vec<_>>().len()
    }

    #[test]
    fn install_t_1() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join("crack.toml"),
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        let _ = super::install(tmp_dir.path(), &dependencies_dir);
        assert!(std::path::Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
    }

    #[test]
    fn install_t_2() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join("crack.toml"),
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "main"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        let _ = super::install(tmp_dir.path(), &dependencies_dir);
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.main.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
    }

    #[test]
    fn install_t_3() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join("crack.toml"),
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "b"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        let _ = super::install(tmp_dir.path(), &dependencies_dir);
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.b.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 2);
    }

    #[test]
    fn install_t_4() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join("crack.toml"),
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        let _ = super::install(tmp_dir.path(), &dependencies_dir);
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.with_dependencies.rolling")
        ));
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 2);
    }

    #[test]
    fn install_t_5() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join("crack.toml"),
            r#"
            name = "package_name"

            [[dependencies.commit]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            commit = "909896f5646b7fd9f058dcd21961b8d5599dec3b"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        let _ = super::install(tmp_dir.path(), &dependencies_dir);
        assert!(Path::exists(&dependencies_dir.join(
            "githubOtherFiles.909896f5646b7fd9f058dcd21961b8d5599dec3b.commit"
        )));
        assert_eq!(nfiles(&dependencies_dir), 1);
    }

    #[test]
    fn install_t_6() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join("crack.toml"),
            r#"
            name = "package_name"

            [[dependencies.commit]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            commit = "a4bf57c513ebc8ed89cc546e8c120c9321357632"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        let _ = super::install(tmp_dir.path(), &dependencies_dir);
        assert!(Path::exists(&dependencies_dir.join(
            "githubOtherFiles.a4bf57c513ebc8ed89cc546e8c120c9321357632.commit"
        )));
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 2);
    }
}
