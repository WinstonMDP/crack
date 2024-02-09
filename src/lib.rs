use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ffi::OsString, fmt::Debug, fs, path::Path, process::Command};

pub const CFG_FILE_NAME: &str = "crack.toml";
const LOCK_FILE_NAME: &str = "crack.lock";

#[derive(Deserialize, Serialize, Debug)]
pub struct Cfg {
    pub name: String,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Commit {
        repo: String,
        commit: String,
    },
    Rolling {
        repo: String,
        branch: Option<String>,
    },
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
) -> Result<Vec<Dependency>> {
    let mut dependencies = install_h(cfg_dir, dependencies_dir, buffer)?;
    dependencies.sort_unstable();
    dependencies.dedup();
    Ok(dependencies)
}

/// Install, but dependencies aren't sorted and deduped.
fn install_h(
    cfg_dir: &Path,
    dependencies_dir: &Path,
    buffer: &mut impl std::io::Write,
) -> Result<Vec<Dependency>> {
    let cfg_path = cfg_dir.join(CFG_FILE_NAME);
    let mut cfg: Cfg = toml::from_str(
        &fs::read_to_string(&cfg_path)
            .with_context(|| format!("Failed with {cfg_path:#?} cfg file."))?,
    )
    .with_context(|| format!("Failed with {cfg_path:#?} cfg file."))?;
    let mut dependencies_to_lock = Vec::new();
    for dependency in &cfg.dependencies {
        let dir_name = dependency_dir(dependency)?;
        let dir_path = dependencies_dir.join(&dir_name);
        if !Path::new(&dir_path).exists() {
            match dependency {
                Dependency::Commit { repo, commit } => {
                    std::fs::create_dir(&dir_path)?;
                    with_stderr_and_context(
                        &Command::new("git")
                            .current_dir(&dir_path)
                            .arg("init")
                            .arg("-q")
                            .output()?,
                        &cfg_path,
                        dependency,
                    )?;
                    with_stderr_and_context(
                        &Command::new("git")
                            .current_dir(&dir_path)
                            .arg("remote")
                            .arg("add")
                            .arg("origin")
                            .arg(repo)
                            .output()?,
                        &cfg_path,
                        dependency,
                    )?;
                    with_stderr_and_context(
                        &Command::new("git")
                            .current_dir(&dir_path)
                            .arg("fetch")
                            .arg("-q")
                            .arg("--depth=1")
                            .arg("origin")
                            .arg(commit)
                            .output()?,
                        &cfg_path,
                        dependency,
                    )?;
                    with_stderr_and_context(
                        &Command::new("git")
                            .current_dir(&dir_path)
                            .arg("checkout")
                            .arg("-q")
                            .arg("FETCH_HEAD")
                            .output()?,
                        &cfg_path,
                        dependency,
                    )?;
                    writeln!(buffer, "{dependency:?} was installed")?;
                }
                Dependency::Rolling { repo, branch } => {
                    let mut command = Command::new("git");
                    let mut command = command
                        .current_dir(dependencies_dir)
                        .arg("clone")
                        .arg("-q")
                        .arg("--depth=1")
                        .arg(repo);
                    if let Some(branch) = &branch {
                        command = command.arg("-b").arg(branch);
                    }
                    with_stderr_and_context(
                        &command.arg(&dir_name).output()?,
                        &cfg_path,
                        dependency,
                    )?;
                    writeln!(buffer, "{dependency:?} was installed")?;
                    dependencies_to_lock.append(&mut install(&dir_path, dependencies_dir, buffer)?);
                }
            }
        }
        dependencies_to_lock.append(&mut install(&dir_path, dependencies_dir, buffer)?);
    }
    dependencies_to_lock.append(&mut cfg.dependencies);
    Ok(dependencies_to_lock)
}

fn with_stderr_and_context(
    output: &std::process::Output,
    cfg: &Path,
    dependency: &Dependency,
) -> Result<()> {
    with_sterr(output).with_context(|| format!("Failed with {cfg:?} cfg file, {dependency:?}."))
}

pub fn with_sterr(output: &std::process::Output) -> Result<()> {
    if !output.stderr.is_empty() {
        anyhow::bail!(std::str::from_utf8(&output.stderr)?.to_string())
    }
    Ok(())
}

/// Delete dependencies directories, which aren't in ``LOCK_FILE_NAME`` file.
pub fn clean(
    locked_dependencies: &[Dependency],
    dependencies_dir: &Path,
    buffer: &mut impl std::io::Write,
) -> Result<()> {
    let mut locked_dependency_dirs = locked_dependencies
        .iter()
        .map(dependency_dir)
        .collect::<Result<Vec<OsString>>>()?;
    locked_dependency_dirs.sort_unstable();
    for file in fs::read_dir(dependencies_dir)? {
        let dir = file
            .with_context(|| format!("Failed with {dependencies_dir:#?} dependencies directory."))?
            .file_name();
        if locked_dependency_dirs.binary_search(&dir).is_err() {
            fs::remove_dir_all(dependencies_dir.join(&dir))
                .with_context(|| format!("Failed with {dir:#?} directory."))?;
            writeln!(buffer, "{dir:#?} was deleted")?;
        }
    }
    Ok(())
}

pub fn dependency_dir(dependency: &Dependency) -> Result<OsString> {
    match dependency {
        Dependency::Commit { repo, commit } => {
            let mut dir = OsString::from(repo_name(repo)?);
            dir.push(".");
            dir.push(commit);
            dir.push(".commit");
            Ok(dir)
        }
        Dependency::Rolling { repo, branch } => {
            let mut dir = OsString::from(repo_name(repo)?);
            dir.push(".");
            dir.push(&branch.clone().unwrap_or("default".to_string()));
            dir.push(".rolling");
            Ok(dir)
        }
    }
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

/// Content of ``LOCK_FILE_NAME`` file.
pub fn locked_dependencies(lock_file_dir: &Path) -> Result<Vec<Dependency>> {
    let lock_file = lock_file_dir.join(LOCK_FILE_NAME);
    if lock_file.exists() {
        toml::from_str::<HashMap<String, Vec<Dependency>>>(
            &fs::read_to_string(&lock_file)
                .with_context(|| format!("Failed with {lock_file:#?} lock file."))?,
        )
        .with_context(|| format!("Failed with {lock_file:#?} lock file."))?
        .remove("dependencies")
        .ok_or_else(|| anyhow!("There isn't dependencies field in {lock_file:#?} lock file."))
    } else {
        Ok(Vec::new())
    }
}

/// Write dependencies to ``LOCK_FILE_NAME`` file.
pub fn lock(lock_dir: &Path, dependencies: &[Dependency]) -> Result<()> {
    let lock_file = lock_dir.join(LOCK_FILE_NAME);
    fs::write(
        &lock_file,
        toml::to_string(&HashMap::from([("dependencies", dependencies)]))
            .with_context(|| format!("Failed with {lock_file:#?} lock file."))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::empty, path::Path};
    use Dependency::{Commit, Rolling};

    #[test]
    fn repo_name_t_1() {
        assert_eq!(
            repo_name("https://github.com/WinstonMDP/repo_name.git").unwrap(),
            "repo_name"
        );
    }

    #[test]
    fn repo_name_t_2() {
        assert_eq!(
            repo_name("ssh://[user@]host.xz[:port]/~[user]/path/to/repo.git/").unwrap(),
            "repo"
        );
    }

    #[test]
    fn repo_name_t_3() {
        assert_eq!(
            repo_name("https://github.com/WinstonMDP/repo-name.git").unwrap(),
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
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![Rolling {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                branch: None
            }]
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
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "main"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![Rolling {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                branch: Some("main".to_string())
            }]
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
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "b"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                },
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: Some("b".to_string())
                }
            ]
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
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                },
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: Some("with_dependencies".to_string())
                }
            ]
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
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            commit = "909896f5646b7fd9f058dcd21961b8d5599dec3b"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![{
                Commit {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    commit: "909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string(),
                }
            }]
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
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            commit = "e636a1ec3659dee39ae935837fb13eea7e7d8faf"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![
                Commit {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    commit: "e636a1ec3659dee39ae935837fb13eea7e7d8faf".to_string()
                },
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                },
            ]
        );
        let commit_dependency_dir = dependencies_dir
            .join("githubOtherFiles.e636a1ec3659dee39ae935837fb13eea7e7d8faf.commit");
        assert!(Path::exists(&commit_dependency_dir));
        assert_eq!(
            fs::read_to_string(commit_dependency_dir.join(CFG_FILE_NAME)).unwrap(),
            r#"name = "otherFiles"

[[dependencies]]
repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
"#
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 2);
    }

    #[test]
    fn install_t_7() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![Rolling {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                branch: None
            },]
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(!Path::exists(
            &dependencies_dir.join("githubOtherFiles.b.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "b"
            "#,
        )
        .unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![Rolling {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                branch: Some("b".to_string())
            },]
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
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![Rolling {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                branch: None
            },]
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![Rolling {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                branch: None
            },]
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
    }

    #[test]
    fn install_t_9() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![Rolling {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                branch: None
            }]
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 1);
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                },
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: Some("with_dependencies".to_string())
                }
            ]
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.with_dependencies.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 2);
    }

    #[test]
    fn install_t_10() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                },
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: Some("with_dependencies".to_string())
                }
            ]
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.with_dependencies.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 2);
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        assert_eq!(
            install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
            vec![
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                },
                Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: Some("with_dependencies".to_string())
                }
            ]
        );
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.with_dependencies.rolling")
        ));
        assert_eq!(nfiles(&dependencies_dir), 2);
    }

    #[test]
    fn clean_t_1() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let cfg = tmp_dir.path().join(CFG_FILE_NAME);
        fs::write(
            &cfg,
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        lock(
            tmp_dir.path(),
            &install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
        )
        .unwrap();
        fs::write(
            cfg,
            r#"
            name = "package_name"
            "#,
        )
        .unwrap();
        lock(
            tmp_dir.path(),
            &install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
        )
        .unwrap();
        clean(
            &locked_dependencies(tmp_dir.path()).unwrap(),
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
        let cfg = tmp_dir.path().join(CFG_FILE_NAME);
        fs::write(
            &cfg,
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "b"
            "#,
        )
        .unwrap();
        let dependencies_dir = tmp_dir.path().join("dependencies");
        fs::create_dir(&dependencies_dir).unwrap();
        lock(
            tmp_dir.path(),
            &install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
        )
        .unwrap();
        assert!(Path::exists(
            &dependencies_dir.join("githubOtherFiles.default.rolling")
        ));
        fs::write(
            cfg,
            r#"
            name = "package_name"

            [[dependencies]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        lock(
            tmp_dir.path(),
            &install(tmp_dir.path(), &dependencies_dir, &mut empty()).unwrap(),
        )
        .unwrap();
        clean(
            &locked_dependencies(tmp_dir.path()).unwrap(),
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
