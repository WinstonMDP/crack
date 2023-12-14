// TODO: more efficient git calls

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(clap::Parser)]
#[command(about = "A Sanskrit package manager", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Subcommand,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Install crack.toml dependencies, which aren't in the directory.
    I,
    /// Update crack.lock dependencies.
    U,
    /// Delete directories, which aren't in the crack.lock.
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
/// Clone only those repositories, which aren't in ``dependencies_dir``.
/// Returns all dependencies, which must be contained in ``dependencies_dir``
/// according to cfg and its transitive dependencies.
pub fn install(
    cfg_dir: &Path,
    dependencies_dir: &Path,
    buffer: &mut impl std::io::Write,
) -> Result<Dependencies> {
    let mut dependencies = install_h(cfg_dir, dependencies_dir, buffer)?;
    dependencies.rolling.sort();
    dependencies.rolling.dedup();
    dependencies.commit.sort();
    dependencies.commit.dedup();
    Ok(dependencies)
}

/// Install, but dependencies aren't sorted and deduped.
fn install_h(
    cfg_dir: &Path,
    dependencies_dir: &Path,
    buffer: &mut impl std::io::Write,
) -> Result<Dependencies> {
    let cfg_path = cfg_dir.join(CFG_FILE_NAME);
    let cfg: Cfg = toml::from_str(
        &fs::read_to_string(&cfg_path)
            .with_context(|| format!("Failed with {cfg_path:#?} cfg file."))?,
    )
    .with_context(|| format!("Failed with {cfg_path:#?} cfg file."))?;
    let mut dependencies_to_lock = Dependencies::default();
    for dependency in &cfg.dependencies.rolling {
        let dir_name = rolling_dependency_dir(dependency)?;
        let dir_path = dependencies_dir.join(&dir_name);
        if !Path::new(&dir_path).exists() {
            let mut command = Command::new("git");
            let mut command = command
                .current_dir(dependencies_dir)
                .arg("clone")
                .arg("-q")
                .arg(&dependency.repo);
            if let Some(branch) = dependency.branch.clone() {
                command = command.arg("-b").arg(branch);
            }
            with_stderr_and_context(&command.arg(&dir_name).output()?, &cfg_path, dependency)?;
            writeln!(buffer, "{dependency:?} was installed")?;
            dependencies_to_lock.append(install(&dir_path, dependencies_dir, buffer)?);
        }
    }
    for dependency in &cfg.dependencies.commit {
        let dir_name = commit_dependency_dir(dependency)?;
        let dir_path = dependencies_dir.join(&dir_name);
        if !Path::new(&dir_path).exists() {
            with_stderr_and_context(
                &Command::new("git")
                    .current_dir(dependencies_dir)
                    .arg("clone")
                    .arg("-q")
                    .arg(&dependency.repo)
                    .arg(&dir_name)
                    .output()?,
                &cfg_path,
                dependency,
            )?;
            with_stderr_and_context(
                &Command::new("git")
                    .current_dir(&dir_path)
                    .arg("checkout")
                    .arg("-q")
                    .arg(&dependency.commit)
                    .output()?,
                &cfg_path,
                dependency,
            )?;
            writeln!(buffer, "{dependency:?} was installed")?;
            dependencies_to_lock.append(install(&dir_path, dependencies_dir, buffer)?);
        }
    }
    dependencies_to_lock.append(cfg.dependencies);
    Ok(dependencies_to_lock)
}

trait Dependency {}
impl Dependency for RollingDependency {}
impl Dependency for CommitDependency {}

fn with_stderr_and_context(
    output: &std::process::Output,
    cfg: &Path,
    dependency: &(impl Dependency + Debug),
) -> Result<()> {
    if !output.stderr.is_empty() {
        return Err(anyhow!(std::str::from_utf8(&output.stderr)?.to_string()))
            .with_context(|| format!("Failed with {cfg:?} cfg file, {dependency:?}."));
    }
    Ok(())
}

/// Delete dependencies directories, which aren't in ``LOCK_FILE_NAME`` file.
pub fn clean(
    locked_dependencies: &Dependencies,
    dependencies_dir: &Path,
    buffer: &mut impl std::io::Write,
) -> Result<()> {
    let mut rolling_locked_dependencies_dirs = locked_dependencies
        .rolling
        .iter()
        .map(rolling_dependency_dir)
        .collect::<Result<Vec<OsString>>>()?;
    rolling_locked_dependencies_dirs.sort();
    let mut commit_locked_dependencies_dirs = locked_dependencies
        .commit
        .iter()
        .map(commit_dependency_dir)
        .collect::<Result<Vec<OsString>>>()?;
    commit_locked_dependencies_dirs.sort();
    for file in fs::read_dir(dependencies_dir)? {
        let dir = file
            .with_context(|| format!("Failed with {dependencies_dir:#?} dependencies directory."))?
            .file_name();
        if rolling_locked_dependencies_dirs
            .binary_search(&dir)
            .is_err()
            && commit_locked_dependencies_dirs.binary_search(&dir).is_err()
        {
            fs::remove_dir_all(dependencies_dir.join(&dir))
                .with_context(|| format!("Failed with {dir:#?} directory."))?;
            writeln!(buffer, "{dir:#?} was deleted")?;
        }
    }
    Ok(())
}

pub fn rolling_dependency_dir(dependency: &RollingDependency) -> Result<OsString> {
    let mut dir = OsString::from(repo_name(&dependency.repo)?);
    dir.push(".");
    dir.push(&dependency.branch.clone().unwrap_or("default".to_string()));
    dir.push(".rolling");
    Ok(dir)
}

fn commit_dependency_dir(dependency: &CommitDependency) -> Result<OsString> {
    let mut dir = OsString::from(repo_name(&dependency.repo)?);
    dir.push(".");
    dir.push(&dependency.commit);
    dir.push(".commit");
    Ok(dir)
}

fn repo_name(git_url: &str) -> Result<&str> {
    Ok(regex::Regex::new(r"/([^/]*)\.git")?
        .captures(git_url)
        .ok_or_else(|| {
            anyhow!("Can't capture repository name in \"{git_url}\". Probably, \".git\" part is missed.")
        })?
        .get(1)
        .ok_or_else(|| {
            anyhow!("Can't capture repository name in \"{git_url}\". Probably, \".git\" part is missed.")
        })?
        .as_str())
}

/// Path of the first ancestor directory (or current directory) containing ``CFG_FILE_NAME`` file.
pub fn project_root() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    current_dir
        .ancestors()
        .find(|x| x.join(CFG_FILE_NAME).exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow!("Can't find {CFG_FILE_NAME} in the current and ancestor directories.")
        })
}

/// Content of ``LOCK_FILE_NAME`` file.
pub fn locked_dependencies(lock_file_dir: &Path) -> Result<Dependencies> {
    let lock_file = lock_file_dir.join(LOCK_FILE_NAME);
    Ok(if lock_file.exists() {
        toml::from_str(
            &fs::read_to_string(&lock_file)
                .with_context(|| format!("Failed with {lock_file:#?} lock file."))?,
        )
        .with_context(|| format!("Failed with {lock_file:#?} lock file."))?
    } else {
        Dependencies::default()
    })
}

/// Write dependencies to ``LOCK_FILE_NAME`` file.
pub fn lock(lock_dir: &Path, dependencies: &Dependencies) -> Result<()> {
    let lock_file = lock_dir.join(LOCK_FILE_NAME);
    fs::write(
        &lock_file,
        toml::to_string(dependencies)
            .with_context(|| format!("Failed with {lock_file:#?} lock file."))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, io::empty, path::Path};

    #[test]
    fn repo_name_t_1() {
        assert_eq!(
            super::repo_name("https://github.com/WinstonMDP/repo_name.git").unwrap(),
            "repo_name"
        );
    }

    #[test]
    fn repo_name_t_2() {
        assert_eq!(
            super::repo_name("ssh://[user@]host.xz[:port]/~[user]/path/to/repo.git/").unwrap(),
            "repo"
        );
    }

    #[test]
    fn repo_name_t_3() {
        assert_eq!(
            super::repo_name("https://github.com/WinstonMDP/repo-name.git").unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            &super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            &super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
        )
        .unwrap();
        super::clean(
            &super::locked_dependencies(tmp_dir.path()).unwrap(),
            &dependencies_dir,
            &mut empty(),
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
            &super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
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
            &super::install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
        )
        .unwrap();
        super::clean(
            &super::locked_dependencies(tmp_dir.path()).unwrap(),
            &dependencies_dir,
            &mut empty(),
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
