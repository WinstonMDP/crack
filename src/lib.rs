// TODO: more efficient git calls

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(clap::Parser)]
#[command(about = "Sanskrit package manager", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Subcommand,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Install crack.toml dependencies, that are not in the directory.
    I,
    /// Update crack.lock dependencies.
    U,
    /// Delete directories, that are not in the crack.lock.
    C,
}

const CFG_FILE_NAME: &str = "crack.toml";
const LOCK_FILE_NAME: &str = "crack.lock";

#[derive(Deserialize, Serialize, Debug)]
pub struct Cfg {
    pub name: String,
    #[serde(default)]
    pub dependencies: Dependencies,
}

#[derive(Deserialize, Serialize, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
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

    fn append_and_sort_and_dedup(&mut self, other: Self) {
        self.append(other);
        self.rolling.sort();
        self.rolling.dedup();
        self.commit.sort();
        self.commit.dedup();
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RollingDependency {
    pub repo: String,
    pub branch: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CommitDependency {
    pub repo: String,
    pub commit: String,
}

/// Clone rolling dependencies in ``<repo_name>.<branch>.rolling`` directories and
/// checkout to the respective branch.
/// Clone commit dependencies in ``<repo_name>.<commit>.commit`` directories and
/// checkout to the respective commits.
/// Clone only those repositories, that are not in ``dependencies_dir``.
/// Returns all dependencies, that must be contained in ``dependencies_dir``
/// according to cfg and its transitive dependencies.
pub fn install(cfg_dir: &Path, dependencies_dir: &Path) -> anyhow::Result<Dependencies> {
    let cfg_path = cfg_dir.join(CFG_FILE_NAME);
    let cfg: Cfg = toml::from_str(
        &fs::read_to_string(&cfg_path)
            .with_context(|| format!("Failed with cfg file {cfg_path:#?}"))?,
    )
    .with_context(|| format!("Failed with cfg file {cfg_path:#?}"))?;
    let mut dependencies_to_lock = Dependencies::default();
    for dependency in &cfg.dependencies.rolling {
        let dir_name = rolling_dependency_dir(dependency);
        let dir_path = dependencies_dir.join(&dir_name);
        if !Path::new(&dir_path).exists() {
            let mut command = Command::new("git");
            let mut command = command
                .current_dir(dependencies_dir)
                .arg("clone")
                .arg(&dependency.repo);
            if let Some(branch) = dependency.branch.clone() {
                command = command.arg("-b").arg(branch);
            }
            command.arg(&dir_name).output()?;
            println!("{dependency:?} was installed");
            dependencies_to_lock.append_and_sort_and_dedup(install(&dir_path, dependencies_dir)?);
        }
    }
    for dependency in &cfg.dependencies.commit {
        let dir_name = commit_dependency_dir(dependency);
        let dir_path = dependencies_dir.join(&dir_name);
        if !Path::new(&dir_path).exists() {
            Command::new("git")
                .current_dir(dependencies_dir)
                .arg("clone")
                .arg(&dependency.repo)
                .arg(&dir_name)
                .output()?;
            Command::new("git")
                .current_dir(&dir_path)
                .arg("checkout")
                .arg(&dependency.commit)
                .output()?;
            println!("{dependency:?} was installed");
            dependencies_to_lock.append_and_sort_and_dedup(install(&dir_path, dependencies_dir)?);
        }
    }
    dependencies_to_lock.append_and_sort_and_dedup(cfg.dependencies);
    Ok(dependencies_to_lock)
}

/// Delete dependencies directories, that are not in ``LOCK_FILE_NAME`` file.
pub fn clean(locked_dependencies: &Dependencies, dependencies_dir: &Path) -> anyhow::Result<()> {
    for file in fs::read_dir(dependencies_dir)? {
        let dir = file
            .with_context(|| format!("Failed with dependencies directory {dependencies_dir:#?}"))?
            .file_name();
        if !locked_dependencies
            .rolling
            .iter()
            .any(|x| rolling_dependency_dir(x) == dir)
            && !locked_dependencies
                .commit
                .iter()
                .any(|x| commit_dependency_dir(x) == dir)
        {
            fs::remove_dir_all(dependencies_dir.join(&dir))
                .with_context(|| format!("Failed with directory {dir:#?}"))?;
        }
    }
    Ok(())
}

#[must_use]
pub fn rolling_dependency_dir(dependency: &RollingDependency) -> OsString {
    let mut dir = OsString::from(repo_name(&dependency.repo));
    dir.push(".");
    dir.push(&dependency.branch.clone().unwrap_or("default".to_string()));
    dir.push(".rolling");
    dir
}

fn commit_dependency_dir(dependency: &CommitDependency) -> OsString {
    let mut dir = OsString::from(repo_name(&dependency.repo));
    dir.push(".");
    dir.push(&dependency.commit);
    dir.push(".commit");
    dir
}

/// ``git_url`` must consist ``.git`` part
fn repo_name(git_url: &str) -> &str {
    regex::Regex::new(r"/([^/]*)\.git")
        .expect("the regex should be valid")
        .captures(git_url)
        .expect("the regex should be right to extract a repository name")
        .get(1)
        .expect("the regex should be right to extract a repository name")
        .as_str()
}

/// Path of the first ancestor directory (or current directory) containing ``CFG_FILE_NAME`` file.
pub fn project_root() -> anyhow::Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    for i in current_dir.ancestors() {
        if i.join(CFG_FILE_NAME).exists() {
            return Ok(i.to_path_buf());
        }
    }
    anyhow::bail!("Can't find {CFG_FILE_NAME} in the current and ancestor directories")
}

/// Content of ``LOCK_FILE_NAME`` file.
pub fn locked_dependencies(lock_file_dir: &Path) -> anyhow::Result<Dependencies> {
    let lock_file = lock_file_dir.join(LOCK_FILE_NAME);
    if lock_file.exists() {
        Ok(toml::from_str(
            &fs::read_to_string(&lock_file)
                .with_context(|| format!("Failed with file {lock_file:#?}"))?,
        )
        .with_context(|| format!("Failed with file {lock_file:#?}"))?)
    } else {
        Ok(Dependencies::default())
    }
}

/// Write dependencies to ``LOCK_FILE_NAME`` file.
pub fn lock(lock_dir: &Path, dependencies: &Dependencies) -> anyhow::Result<()> {
    let lock_file = lock_dir.join(LOCK_FILE_NAME);
    fs::write(
        &lock_file,
        toml::to_string(dependencies)
            .with_context(|| format!("Failed with lock file {lock_file:#?}"))?,
    )?;
    Ok(())
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
            tmp_dir.path().join(super::CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![super::RollingDependency {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                commit: vec![]
            }
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
    }

    #[test]
    fn install_t_2() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(super::CFG_FILE_NAME),
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
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![super::RollingDependency {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: Some("main".to_string())
                }],
                commit: vec![]
            }
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.main.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
    }

    #[test]
    fn install_t_3() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(super::CFG_FILE_NAME),
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
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![
                    super::RollingDependency {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: None
                    },
                    super::RollingDependency {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("b".to_string())
                    }
                ],
                commit: vec![]
            }
        );
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
            tmp_dir.path().join(super::CFG_FILE_NAME),
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
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![
                    super::RollingDependency {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: None
                    },
                    super::RollingDependency {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("with_dependencies".to_string())
                    }
                ],
                commit: vec![]
            }
        );
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
            tmp_dir.path().join(super::CFG_FILE_NAME),
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
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![],
                commit: vec![super::CommitDependency {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    commit: "909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string()
                }]
            }
        );
        assert!(Path::exists(&dependencies_dir.join(
            "githubOtherFiles.909896f5646b7fd9f058dcd21961b8d5599dec3b.commit"
        )));
        assert_eq!(nfiles(&dependencies_dir), 1);
    }

    #[test]
    fn install_t_6() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(super::CFG_FILE_NAME),
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
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![super::RollingDependency {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                commit: vec![super::CommitDependency {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    commit: "a4bf57c513ebc8ed89cc546e8c120c9321357632".to_string()
                }]
            }
        );
        assert!(Path::exists(&dependencies_dir.join(
            "githubOtherFiles.a4bf57c513ebc8ed89cc546e8c120c9321357632.commit"
        )));
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 2);
    }

    #[test]
    fn install_t_7() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(super::CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![super::RollingDependency {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                commit: vec![]
            }
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(!Path::exists(
            &dependencies_dir.join("githubOtherFiles.b.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
        fs::write(
            tmp_dir.path().join(super::CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "b"
            "#,
        )
        .unwrap();
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![super::RollingDependency {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: Some("b".to_string())
                }],
                commit: vec![]
            }
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.b.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 2);
    }

    #[test]
    fn install_t_8() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(super::CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![super::RollingDependency {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                commit: vec![]
            }
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
        assert_eq!(
            super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
            super::Dependencies {
                rolling: vec![super::RollingDependency {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                commit: vec![]
            }
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
    }

    #[test]
    fn clean_t_1() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let cfg = tmp_dir.path().join(super::CFG_FILE_NAME);
        fs::write(
            &cfg,
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        super::lock(
            tmp_dir.path(),
            &super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
        )
        .unwrap();
        fs::write(
            cfg,
            r#"
            name = "package_name"
            "#,
        )
        .unwrap();
        super::lock(
            tmp_dir.path(),
            &super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
        )
        .unwrap();
        super::clean(
            &super::locked_dependencies(tmp_dir.path()).unwrap(),
            &dependencies_dir,
        )
        .unwrap();
        assert!(!Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 0);
    }

    #[test]
    fn clean_t_2() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let cfg = tmp_dir.path().join(super::CFG_FILE_NAME);
        fs::write(
            &cfg,
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
        super::lock(
            tmp_dir.path(),
            &super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
        )
        .unwrap();
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        fs::write(
            cfg,
            r#"
            name = "package_name"

            [[dependencies.rolling]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        super::lock(
            tmp_dir.path(),
            &super::install(tmp_dir.path(), &dependencies_dir).unwrap(),
        )
        .unwrap();
        super::clean(
            &super::locked_dependencies(tmp_dir.path()).unwrap(),
            &dependencies_dir,
        )
        .unwrap();
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(!Path::exists(
            &dependencies_dir.join("githubOtherFiles.b.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
    }
}
