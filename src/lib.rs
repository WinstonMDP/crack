use anyhow::{anyhow, Context, Result};
use bimap::BiMap;
use petgraph::{prelude::NodeIndex, Graph};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    fmt::Debug,
    fs,
    path::Path,
    process::Command,
};

pub const CFG_FILE_NAME: &str = "crack.toml";
const LOCK_FILE_NAME: &str = "crack.lock";

#[derive(Deserialize, Serialize, Debug)]
pub struct Cfg {
    #[serde(default)]
    pub deps: Vec<Dep>,
    pub name: String,
}

impl Cfg {
    fn from(path: &Path) -> Result<Cfg> {
        toml::from_str(
            &fs::read_to_string(path)
                .with_context(|| format!("Failed with {path:#?} cfg file."))?,
        )
        .with_context(|| format!("Failed with {path:#?} cfg file."))
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct Dep {
    #[serde(flatten)]
    pub lock_unit: LockUnit,
    name: Option<String>,
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
pub enum LockUnit {
    Commit {
        commit: String,
        repo: String,
    },
    Rolling {
        branch: Option<String>,
        repo: String,
    },
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct BuildUnit {
    dir: OsString,
    name_map: BTreeMap<String, OsString>,
}

/// Clone rolling deps in ``<repo_name>.rolling.<branch>`` directories and
/// checkout to the respective branch.
/// Clone commit deps in ``<repo_name>.commit.<commit>`` directories and
/// checkout to the respective commits.
/// Clone only those repositories, which aren't in ``deps_dir``.
/// Returns all deps, which must be contained in ``deps_dir``
/// according to cfg and its transitive deps.
#[allow(clippy::missing_panics_doc, clippy::cast_possible_truncation)]
pub fn install(
    cfg_dir: &Path,
    deps_dir: &Path,
    buffer: &mut impl std::io::Write,
) -> Result<(Vec<LockUnit>, Vec<Vec<BuildUnit>>)> {
    let mut graph: Graph<(), ()> = Graph::new();
    let mut i_bimap = BiMap::new();
    let cfg_path = cfg_dir.join(CFG_FILE_NAME);
    let mut deps = vec![];
    install_h(
        Cfg::from(&cfg_path)?.deps,
        deps_dir,
        &cfg_path,
        buffer,
        OsString::from("root"),
        NodeIndex::from(0),
        &mut i_bimap,
        &mut graph,
        &mut deps,
    )?;
    let sccs = petgraph::algo::kosaraju_scc(&graph)
        .iter()
        .map(|x| {
            x.iter()
                .map(|y| i_bimap.remove_by_right(y).unwrap().0)
                .collect()
        })
        .collect();
    deps.sort_unstable();
    deps.dedup();
    Ok((deps, sccs))
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn install_h(
    deps: Vec<Dep>,
    deps_dir: &Path,
    cfg_path: &Path,
    buffer: &mut impl std::io::Write,
    dir_name: OsString,
    prev_i: NodeIndex,
    i_bimap: &mut BiMap<BuildUnit, NodeIndex>,
    graph: &mut Graph<(), ()>,
    installed_locks: &mut Vec<LockUnit>,
) -> Result<()> {
    let mut vec_for_name_map = vec![];
    let mut vec_to_trans_deps_install = vec![];
    for dep in deps {
        let dir_name = dep_dir(&dep.lock_unit)?;
        let dir_path = deps_dir.join(&dir_name);
        if !Path::new(&dir_path).exists() {
            match dep.lock_unit {
                LockUnit::Commit {
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
                        cfg_path,
                        &dep.lock_unit,
                    )?;
                    with_stderr_and_context(
                        &Command::new("git")
                            .current_dir(&dir_path)
                            .arg("remote")
                            .arg("add")
                            .arg("origin")
                            .arg(repo)
                            .output()?,
                        cfg_path,
                        &dep.lock_unit,
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
                        cfg_path,
                        &dep.lock_unit,
                    )?;
                    with_stderr_and_context(
                        &Command::new("git")
                            .current_dir(&dir_path)
                            .arg("checkout")
                            .arg("-q")
                            .arg("FETCH_HEAD")
                            .output()?,
                        cfg_path,
                        &dep.lock_unit,
                    )?;
                    writeln!(buffer, "{:?} was installed", dep.lock_unit)?;
                }
                LockUnit::Rolling {
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
                    with_stderr_and_context(
                        &command.arg(&dir_name).output()?,
                        cfg_path,
                        &dep.lock_unit,
                    )?;
                    writeln!(buffer, "{:?} was installed", dep.lock_unit)?;
                }
            }
        }
        let dep_cfg = Cfg::from(&dir_path.join(CFG_FILE_NAME))?;
        vec_for_name_map.push((dep_cfg.name, dep.name, dir_name.clone()));
        vec_to_trans_deps_install.push((dep_cfg.deps, dir_name, dir_path));
        installed_locks.push(dep.lock_unit);
    }
    let vec_for_name_map: Vec<(String, OsString)> = vec_for_name_map
        .into_iter()
        .map(|x| (if let Some(name) = x.1 { name } else { x.0 }, x.2))
        .collect();
    let mut name_map = BTreeMap::new();
    for (name, dir) in vec_for_name_map {
        anyhow::ensure!(!name_map.contains_key(&name));
        name_map.insert(name, dir);
    }
    let build_unit = BuildUnit {
        dir: dir_name,
        name_map,
    };
    let (i, contains_edge) = if i_bimap.contains_left(&build_unit) {
        let i = *i_bimap.get_by_left(&build_unit).unwrap();
        (i, graph.contains_edge(prev_i, i))
    } else {
        let i = graph.add_node(());
        i_bimap.insert(build_unit, i);
        (i, false)
    };
    if !contains_edge {
        graph.add_edge(prev_i, i, ());
        for (deps, dep_dir_name, dep_dir_path) in vec_to_trans_deps_install {
            install_h(
                deps,
                deps_dir,
                &dep_dir_path,
                buffer,
                dep_dir_name,
                i,
                i_bimap,
                graph,
                installed_locks,
            )?;
        }
    }
    Ok(())
}

fn with_stderr_and_context(
    output: &std::process::Output,
    cfg: &Path,
    dep: &LockUnit,
) -> Result<()> {
    with_sterr(output).with_context(|| format!("Failed with {cfg:?} cfg file, {dep:?}."))
}

pub fn with_sterr(output: &std::process::Output) -> Result<()> {
    if !output.stderr.is_empty() {
        anyhow::bail!(std::str::from_utf8(&output.stderr)?.to_string())
    }
    Ok(())
}

/// Delete deps directories, which aren't in ``LOCK_FILE_NAME`` file.
pub fn clean(
    locked_deps: &[LockUnit],
    deps_dir: &Path,
    buffer: &mut impl std::io::Write,
) -> Result<()> {
    let locked_dep_dirs = locked_deps
        .iter()
        .map(dep_dir)
        .collect::<Result<Vec<OsString>>>()?;
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

pub fn dep_dir(dep: &LockUnit) -> Result<OsString> {
    match dep {
        LockUnit::Commit { repo, commit } => {
            let mut dir = OsString::from(repo_name(repo)?);
            dir.push(".commit");
            dir.push(".");
            dir.push(commit);
            Ok(dir)
        }
        LockUnit::Rolling { repo, branch } => {
            let mut dir = OsString::from(repo_name(repo)?);
            dir.push(".rolling");
            dir.push(".");
            dir.push(&branch.clone().unwrap_or("default".to_string()));
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
pub fn locked_deps(lock_file_dir: &Path) -> Result<Vec<LockUnit>> {
    let lock_file = lock_file_dir.join(LOCK_FILE_NAME);
    if lock_file.exists() {
        toml::from_str::<HashMap<String, Vec<LockUnit>>>(
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
pub fn lock(lock_dir: &Path, deps: &[LockUnit]) -> Result<()> {
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
    use LockUnit::{Commit, Rolling};

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
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }]
                ],
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
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
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.main"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.main")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.main")
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
            name = "name_for_b"
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
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.b"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([
                            (
                                "otherFiles".to_string(),
                                OsString::from("githubOtherFiles.rolling.default"),
                            ),
                            (
                                "name_for_b".to_string(),
                                OsString::from("githubOtherFiles.rolling.b")
                            )
                        ])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
        ));
        assert!(Path::exists(&deps_dir.join("githubOtherFiles.rolling.b")));
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
                        branch: None
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("with_dependencies".to_string())
                    },
                ],
                vec![
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.with_dependencies"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherDependencies".to_string(),
                            OsString::from("githubOtherFiles.rolling.with_dependencies")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.with_dependencies")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
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
                    vec![BuildUnit {
                        dir: OsString::from(
                            "githubOtherFiles.commit.909896f5646b7fd9f058dcd21961b8d5599dec3b"
                        ),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from(
                                "githubOtherFiles.commit.909896f5646b7fd9f058dcd21961b8d5599dec3b"
                            )
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(&deps_dir.join(
            "githubOtherFiles.commit.909896f5646b7fd9f058dcd21961b8d5599dec3b"
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
            name = "commit_package"
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
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from(
                            "githubOtherFiles.commit.30cfb86f4e76810eedc1d8d57167289a2b63b4ac"
                        ),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "commit_package".to_string(),
                            OsString::from(
                                "githubOtherFiles.commit.30cfb86f4e76810eedc1d8d57167289a2b63b4ac"
                            )
                        )])
                    }]
                ]
            )
        );
        let commit_dep_dir =
            deps_dir.join("githubOtherFiles.commit.30cfb86f4e76810eedc1d8d57167289a2b63b4ac");
        assert!(Path::exists(&commit_dep_dir));
        assert_eq!(
            fs::read_to_string(commit_dep_dir.join(CFG_FILE_NAME)).unwrap(),
            r#"name = "otherFiles"

[[deps]]
repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
"#
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
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
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
        ));
        assert!(!Path::exists(&deps_dir.join("githubOtherFiles.rolling.b")));
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
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.b"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.b")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
        ));
        assert!(Path::exists(&deps_dir.join("githubOtherFiles.rolling.b")));
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
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
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
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
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
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
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
                        branch: None
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("with_dependencies".to_string())
                    },
                ],
                vec![
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.with_dependencies"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherDependencies".to_string(),
                            OsString::from("githubOtherFiles.rolling.with_dependencies")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.with_dependencies")
        ));
        assert_eq!(nfiles(&deps_dir), 2);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
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
                        branch: None
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("with_dependencies".to_string())
                    },
                ],
                vec![
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.with_dependencies"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherDependencies".to_string(),
                            OsString::from("githubOtherFiles.rolling.with_dependencies")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.with_dependencies")
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
                        branch: None
                    },
                    Rolling {
                        repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                        branch: Some("with_dependencies".to_string())
                    },
                ],
                vec![
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.default"),
                        name_map: BTreeMap::new()
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("githubOtherFiles.rolling.with_dependencies"),
                        name_map: BTreeMap::from([(
                            "otherFiles".to_string(),
                            OsString::from("githubOtherFiles.rolling.default")
                        )])
                    }],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "otherDependencies".to_string(),
                            OsString::from("githubOtherFiles.rolling.with_dependencies")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.default")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.with_dependencies")
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
            name = "cycle"
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
                        BuildUnit {
                            dir: OsString::from("githubOtherFiles.rolling.cyclic_1"),
                            name_map: BTreeMap::from([(
                                "otherFiles".to_string(),
                                OsString::from("githubOtherFiles.rolling.cyclic_2")
                            )])
                        },
                        BuildUnit {
                            dir: OsString::from("githubOtherFiles.rolling.cyclic_2"),
                            name_map: BTreeMap::from([(
                                "otherFiles".to_string(),
                                OsString::from("githubOtherFiles.rolling.cyclic_1")
                            )])
                        },
                    ],
                    vec![BuildUnit {
                        dir: OsString::from("root"),
                        name_map: BTreeMap::from([(
                            "cycle".to_string(),
                            OsString::from("githubOtherFiles.rolling.cyclic_1")
                        )])
                    }]
                ]
            )
        );
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.cyclic_1")
        ));
        assert!(Path::exists(
            &deps_dir.join("githubOtherFiles.rolling.cyclic_2")
        ));
        assert_eq!(nfiles(&deps_dir), 2);
    }

    #[test]
    fn install_t_12() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let cfg = tmp_dir.path().join(CFG_FILE_NAME);
        fs::write(
            cfg,
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
        install(tmp_dir.path(), &deps_dir, &mut empty()).unwrap_err();
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
            &deps_dir.join("githubOtherFiles.rolling.default")
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
            name = "package_name_to_clean"
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
            &deps_dir.join("githubOtherFiles.rolling.default")
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
            &deps_dir.join("githubOtherFiles.rolling.default")
        ));
        assert!(!Path::exists(&deps_dir.join("githubOtherFiles.b.rolling")));
        assert_eq!(nfiles(&deps_dir), 1);
    }
}
