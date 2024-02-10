use anyhow::{anyhow, Context, Result};
use bimap::BiMap;
use petgraph::{prelude::NodeIndex, Graph};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ffi::OsString, fmt::Debug, fs, path::Path, process::Command};

pub const CFG_FILE_NAME: &str = "crack.toml";
const LOCK_FILE_NAME: &str = "crack.lock";

#[derive(Deserialize, Serialize, Debug)]
pub struct Cfg {
    pub name: String,
    #[serde(default)]
    pub deps: Vec<Dep>,
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
pub enum Dep {
    Commit {
        repo: String,
        commit: String,
    },
    Rolling {
        repo: String,
        branch: Option<String>,
    },
}

/// Clone rolling deps in ``<repo_name>.<branch>.rolling`` directories and
/// checkout to the respective branch.
/// Clone commit deps in ``<repo_name>.<commit>.commit`` directories and
/// checkout to the respective commits.
/// Clone only those repositories, which aren't in ``deps_dir``.
/// Returns all deps, which must be contained in ``deps_dir``
/// according to cfg and its transitive deps.
#[allow(clippy::missing_panics_doc)]
pub fn install(
    cfg_dir: &Path,
    deps_dir: &Path,
    buffer: &mut impl std::io::Write,
) -> Result<(Vec<Dep>, Vec<Vec<OsString>>)> {
    let mut graph: Graph<(), ()> = Graph::new();
    let root_index = graph.add_node(());
    let mut index_bimap = BiMap::new();
    index_bimap.insert(OsString::from("root"), root_index);
    let mut deps = vec![];
    install_h(
        cfg_dir,
        deps_dir,
        buffer,
        &mut index_bimap,
        &mut graph,
        root_index,
        &mut deps,
    )?;
    let sccs = petgraph::algo::kosaraju_scc(&graph)
        .iter()
        .map(|x| {
            x.iter()
                .map(|y| index_bimap.remove_by_right(y).unwrap().0)
                .collect()
        })
        .collect();
    Ok((deps, sccs))
}

// TODO: dir name must be made of package name, not repo
// TODO: dir name must be reordered

fn install_h(
    cfg_dir: &Path,
    deps_dir: &Path,
    buffer: &mut impl std::io::Write,
    i_bimap: &mut BiMap<OsString, NodeIndex>,
    graph: &mut Graph<(), ()>,
    current_i: NodeIndex,
    deps: &mut Vec<Dep>,
) -> Result<()> {
    let cfg_path = cfg_dir.join(CFG_FILE_NAME);
    let cfg: Cfg = toml::from_str(
        &fs::read_to_string(&cfg_path)
            .with_context(|| format!("Failed with {cfg_path:#?} cfg file."))?,
    )
    .with_context(|| format!("Failed with {cfg_path:#?} cfg file."))?;
    for dep in cfg.deps {
        let dir_name = dep_dir(&dep)?;
        let dir_path = deps_dir.join(&dir_name);
        if !Path::new(&dir_path).exists() {
            match dep {
                Dep::Commit {
                    ref repo,
                    ref commit,
                } => {
                    std::fs::create_dir(&dir_path)?;
                    with_stderr_and_context(
                        &Command::new("git")
                            .current_dir(&dir_path)
                            .arg("init")
                            .arg("-q")
                            .output()?,
                        &cfg_path,
                        &dep,
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
                        &dep,
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
                        &dep,
                    )?;
                    with_stderr_and_context(
                        &Command::new("git")
                            .current_dir(&dir_path)
                            .arg("checkout")
                            .arg("-q")
                            .arg("FETCH_HEAD")
                            .output()?,
                        &cfg_path,
                        &dep,
                    )?;
                    writeln!(buffer, "{dep:?} was installed")?;
                }
                Dep::Rolling {
                    ref repo,
                    ref branch,
                } => {
                    let mut command = Command::new("git");
                    let mut command = command
                        .current_dir(deps_dir)
                        .arg("clone")
                        .arg("-q")
                        .arg("--depth=1")
                        .arg(repo);
                    if let Some(branch) = &branch {
                        command = command.arg("-b").arg(branch);
                    }
                    with_stderr_and_context(&command.arg(&dir_name).output()?, &cfg_path, &dep)?;
                    writeln!(buffer, "{dep:?} was installed")?;
                }
            }
        }
        let mut contains_edge = false;
        let dep_i = if i_bimap.contains_left(&dir_name) {
            let dep_i = *i_bimap.get_by_left(&dir_name).unwrap();
            contains_edge = graph.contains_edge(current_i, dep_i);
            dep_i
        } else {
            deps.push(dep);
            let dep_i = graph.add_node(());
            i_bimap.insert(dir_name, dep_i);
            dep_i
        };
        if !contains_edge {
            graph.add_edge(current_i, dep_i, ());
            install_h(&dir_path, deps_dir, buffer, i_bimap, graph, dep_i, deps)?;
        }
    }
    Ok(())
}

fn with_stderr_and_context(output: &std::process::Output, cfg: &Path, dep: &Dep) -> Result<()> {
    with_sterr(output).with_context(|| format!("Failed with {cfg:?} cfg file, {dep:?}."))
}

pub fn with_sterr(output: &std::process::Output) -> Result<()> {
    if !output.stderr.is_empty() {
        anyhow::bail!(std::str::from_utf8(&output.stderr)?.to_string())
    }
    Ok(())
}

/// Delete deps directories, which aren't in ``LOCK_FILE_NAME`` file.
pub fn clean(locked_deps: &[Dep], deps_dir: &Path, buffer: &mut impl std::io::Write) -> Result<()> {
    let mut locked_dep_dirs = locked_deps
        .iter()
        .map(dep_dir)
        .collect::<Result<Vec<OsString>>>()?;
    locked_dep_dirs.sort_unstable();
    for file in fs::read_dir(deps_dir)? {
        let dir = file
            .with_context(|| format!("Failed with {deps_dir:#?} deps directory."))?
            .file_name();
        if locked_dep_dirs.binary_search(&dir).is_err() {
            fs::remove_dir_all(deps_dir.join(&dir))
                .with_context(|| format!("Failed with {dir:#?} directory."))?;
            writeln!(buffer, "{dir:#?} was deleted")?;
        }
    }
    Ok(())
}

pub fn dep_dir(dep: &Dep) -> Result<OsString> {
    match dep {
        Dep::Commit { repo, commit } => {
            let mut dir = OsString::from(repo_name(repo)?);
            dir.push(".");
            dir.push(commit);
            dir.push(".commit");
            Ok(dir)
        }
        Dep::Rolling { repo, branch } => {
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
pub fn locked_deps(lock_file_dir: &Path) -> Result<Vec<Dep>> {
    let lock_file = lock_file_dir.join(LOCK_FILE_NAME);
    if lock_file.exists() {
        toml::from_str::<HashMap<String, Vec<Dep>>>(
            &fs::read_to_string(&lock_file)
                .with_context(|| format!("Failed with {lock_file:#?} lock file."))?,
        )
        .with_context(|| format!("Failed with {lock_file:#?} lock file."))?
        .remove("deps")
        .ok_or_else(|| anyhow!("There isn't deps field in {lock_file:#?} lock file."))
    } else {
        Ok(Vec::new())
    }
}

/// Write deps to ``LOCK_FILE_NAME`` file.
pub fn lock(lock_dir: &Path, deps: &[Dep]) -> Result<()> {
    let lock_file = lock_dir.join(LOCK_FILE_NAME);
    fs::write(
        &lock_file,
        toml::to_string(&HashMap::from([("deps", deps)]))
            .with_context(|| format!("Failed with {lock_file:#?} lock file."))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::empty, path::Path};
    use Dep::{Commit, Rolling};

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

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 1);
    }

    #[test]
    fn install_t_2() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "main"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: Some("main".to_string())
                }],
                vec![
                    vec![OsString::from("githubOtherFiles.main.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.main.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 1);
    }

    #[test]
    fn install_t_3() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "b"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: None
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("b".to_string())
                    }
                ],
                vec![
                    vec![OsString::from("githubOtherFiles.b.rolling")],
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(&deps_dir.join("githubOtherFiles.b.rolling")));
        assert_eq!(nfiles(&deps_dir), 2);
    }

    #[test]
    fn install_t_4() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("with_dependencies".to_string())
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: None
                    },
                ],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("githubOtherFiles.with_dependencies.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.with_dependencies.rolling")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 2);
    }

    #[test]
    fn install_t_5() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            commit = "909896f5646b7fd9f058dcd21961b8d5599dec3b"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![{
                    Commit {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        commit: "909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string(),
                    }
                }],
                vec![
                    vec![OsString::from(
                        "githubOtherFiles.909896f5646b7fd9f058dcd21961b8d5599dec3b.commit"
                    )],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(&deps_dir.join(
            "githubOtherFiles.909896f5646b7fd9f058dcd21961b8d5599dec3b.commit"
        )));
        assert_eq!(nfiles(&deps_dir), 1);
    }

    #[test]
    fn install_t_6() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            commit = "30cfb86f4e76810eedc1d8d57167289a2b63b4ac"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![
                    Commit {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        commit: "30cfb86f4e76810eedc1d8d57167289a2b63b4ac".to_string()
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: None
                    },
                ],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from(
                        "githubOtherFiles.30cfb86f4e76810eedc1d8d57167289a2b63b4ac.commit"
                    )],
                    vec![OsString::from("root")]
                ]
            )
        );
        let commit_dep_dir =
            deps_dir.join("githubOtherFiles.30cfb86f4e76810eedc1d8d57167289a2b63b4ac.commit");
        assert!(Path::exists(&commit_dep_dir));
        assert_eq!(
            fs::read_to_string(commit_dep_dir.join(CFG_FILE_NAME)).unwrap(),
            r#"name = "otherFiles"

[[deps]]
repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
"#
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 2);
    }

    #[test]
    fn install_t_7() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(!Path::exists(&deps_dir.join("githubOtherFiles.b.rolling")));
        assert_eq!(nfiles(&deps_dir), 1);
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "b"
            "#,
        )
        .unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: Some("b".to_string())
                }],
                vec![
                    vec![OsString::from("githubOtherFiles.b.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(&deps_dir.join("githubOtherFiles.b.rolling")));
        assert_eq!(nfiles(&deps_dir), 2);
    }

    #[test]
    fn install_t_8() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 1);
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 1);
    }

    #[test]
    fn install_t_9() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![Rolling {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    branch: None
                }],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 1);
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("with_dependencies".to_string())
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: None
                    },
                ],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("githubOtherFiles.with_dependencies.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.with_dependencies.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 2);
    }

    #[test]
    fn install_t_10() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("with_dependencies".to_string())
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: None
                    },
                ],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("githubOtherFiles.with_dependencies.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.with_dependencies.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 2);
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "with_dependencies"
            "#,
        )
        .unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("with_dependencies".to_string())
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: None
                    },
                ],
                vec![
                    vec![OsString::from("githubOtherFiles.default.rolling")],
                    vec![OsString::from("githubOtherFiles.with_dependencies.rolling")],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.with_dependencies.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 2);
    }

    #[test]
    fn install_t_11() {
        let tmp_dir = tempfile::tempdir().unwrap();
        fs::write(
            tmp_dir.path().join(CFG_FILE_NAME),
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "cyclic_1"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        assert_eq!(
            install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap(),
            (
                vec![
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("cyclic_1".to_string())
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("cyclic_2".to_string())
                    },
                ],
                vec![
                    vec![
                        OsString::from("githubOtherFiles.cyclic_1.rolling"),
                        OsString::from("githubOtherFiles.cyclic_2.rolling")
                    ],
                    vec![OsString::from("root")]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.cyclic_1.rolling")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.cyclic_2.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 2);
    }

    #[test]
    fn clean_t_1() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let cfg = tmp_dir.path().join(CFG_FILE_NAME);
        fs::write(
            &cfg,
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        lock(
            tmp_dir.path(),
            &install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap().0,
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
            &install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap().0,
        )
        .unwrap();
        clean(
            &locked_deps(tmp_dir.path()).unwrap(),
            &deps_dir,
            &mut empty(),
        )
        .unwrap();
        assert!(!Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert_eq!(nfiles(&deps_dir), 0);
    }

    #[test]
    fn clean_t_2() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let cfg = tmp_dir.path().join(CFG_FILE_NAME);
        fs::write(
            &cfg,
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            branch = "b"
            "#,
        )
        .unwrap();
        let deps_dir = tmp_dir.path().join("deps");
        fs::create_dir(&deps_dir).unwrap();
        lock(
            tmp_dir.path(),
            &install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap().0,
        )
        .unwrap();
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        fs::write(
            cfg,
            r#"
            name = "package_name"

            [[deps]]
            repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
            "#,
        )
        .unwrap();
        lock(
            tmp_dir.path(),
            &install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap().0,
        )
        .unwrap();
        clean(
            &locked_deps(tmp_dir.path()).unwrap(),
            &deps_dir,
            &mut empty(),
        )
        .unwrap();
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.default.rolling")
        ));
        assert!(!Path::exists(&deps_dir.join("githubOtherFiles.b.rolling")));
        assert_eq!(nfiles(&deps_dir), 1);
    }
}
