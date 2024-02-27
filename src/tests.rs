use super::*;
use std::{fs, io::empty, path::Path};
use LockType::{Branch, Commit};

#[test]
fn repo_author_and_name_t_1() {
    assert_eq!(
        repo_author_and_name("https://github.com/WinstonMDP/repo_name.git").unwrap(),
        "WinstonMDP.repo_name"
    );
}

#[test]
fn repo_author_and_name_t_2() {
    assert_eq!(
        repo_author_and_name("https://github.com/WinstonMDP/repo-name.git").unwrap(),
        "WinstonMDP.repo-name"
    );
}

#[test]
fn net_installer_t_1() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let deps_dir = tmp_dir.path().join("deps");
    std::fs::create_dir(&deps_dir).unwrap();
    net_installer(
        Path::new("not important"),
        &deps_dir,
        &deps_dir.join("hey_dir"),
        &OsString::from("hey_dir"),
        &LockUnit {
            repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            lock_type: LockType::Branch("b".to_string()),
        },
        &mut empty(),
    )
    .unwrap();
    assert!(Path::exists(&deps_dir.join("hey_dir")));
    assert_eq!(nfiles(&deps_dir), 1);
    assert_eq!(nfiles(&deps_dir.join("hey_dir")), 4);
    assert!(&deps_dir.join("hey_dir").join("test").exists());
}

#[test]
fn net_installer_t_2() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let deps_dir = tmp_dir.path().join("deps");
    std::fs::create_dir(&deps_dir).unwrap();
    net_installer(
        Path::new("not important"),
        &deps_dir,
        &deps_dir.join("hey_dir"),
        &OsString::from("hey_dir"),
        &LockUnit {
            repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            lock_type: LockType::Commit("909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string()),
        },
        &mut empty(),
    )
    .unwrap();
    assert!(Path::exists(&deps_dir.join("hey_dir")));
    assert_eq!(nfiles(&deps_dir), 1);
    assert_eq!(nfiles(&deps_dir.join("hey_dir")), 4);
    assert!(&deps_dir.join("hey_dir").join("test").exists());
}

#[test]
fn tags_t_1() {
    assert_eq!(
        tags("https://github.com/WinstonMDP/githubOtherFiles.git").unwrap(),
        [
            (
                Version::new(0, 3, 5),
                "e636a1ec3659dee39ae935837fb13eea7e7d8faf".to_string()
            ),
            (
                Version::new(1, 3, 5),
                "909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string()
            ),
            (
                Version::new(2, 0, 1),
                "a4bf57c513ebc8ed89cc546e8c120c9321357632".to_string()
            )
        ]
    );
}

fn stub_installer(
    _cfg_path: &Path,
    _deps_dir: &Path,
    dep_dir_path: &Path,
    _dep_dir_name: &OsString,
    lock: &LockUnit,
    _buffer: &mut impl std::io::Write,
) -> Result<()> {
    if !Path::new(dep_dir_path).exists() {
        std::fs::create_dir(dep_dir_path)?;
        fs::write(
            dep_dir_path.join(CFG_FILE_NAME),
            match &lock.lock_type {
                Branch(branch) => match branch.as_str() {
                    "default" | "main" | "b" => r#"name = "otherFiles""#,
                    "with_dependencies" => {
                        r#"
                        name = "otherDependencies"

                        [[deps]]
                        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
                    "#
                    }
                    "cyclic_1" => {
                        r#"
                        name = "otherFiles"

                        [[deps]]
                        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
                        branch = "cyclic_2"
                    "#
                    }
                    "cyclic_2" => {
                        r#"
                        name = "otherFiles"

                        [[deps]]
                        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
                        branch = "cyclic_1"
                    "#
                    }
                    "dev_dep_deps" => {
                        r#"
                        name = "otherFiles"

                        [[deps]]
                        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
                        branch = "dev_dep"
                    "#
                    }
                    "dev_dep" => {
                        r#"
                        name = "otherFiles"

                        [[dev_deps]]
                        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
                    "#
                    }
                    "optional_branch" => {
                        r#"
                        name = "otherFiles"

                        [[deps]]
                        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
                        optional = "option"
                        "#
                    }
                    _ => todo!(),
                },
                Commit(commit) => match commit.as_str() {
                    "30cfb86f4e76810eedc1d8d57167289a2b63b4ac" => {
                        r#"
                        name = "otherFiles"

                        [[deps]]
                        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
                    "#
                    }
                    "909896f5646b7fd9f058dcd21961b8d5599dec3b" => r#"name = "otherFiles""#,
                    _ => todo!(),
                },
            },
        )
        .unwrap();
    };
    Ok(())
}

fn nfiles(dir: &Path) -> usize {
    dir.read_dir().unwrap().collect::<Vec<_>>().len()
}

fn build_file(dir: &Path) -> Vec<Vec<BuildUnit>> {
    serde_json::from_str(&fs::read_to_string(dir.join("crack.build")).unwrap()).unwrap()
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![LockUnit {
            repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            lock_type: Branch("default".to_string())
        }],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.default")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
    ));
    assert_eq!(nfiles(&deps_dir), 1);
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![LockUnit {
            repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            lock_type: Branch("default".to_string())
        }],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.default")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![LockUnit {
            repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            lock_type: Branch("main".to_string())
        }],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.main"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.main")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.main")
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("b".to_string())
            },
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
            },
        ],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.b"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([
                    (
                        "otherFiles".to_string(),
                        OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                    ),
                    (
                        "name_for_b".to_string(),
                        OsString::from("WinstonMDP.githubOtherFiles.branch.b")
                    )
                ])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
    ));
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.b")
    ));
    assert_eq!(nfiles(&deps_dir), 2);
}

#[test]
#[allow(clippy::too_many_lines)]
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
            },
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("with_dependencies".to_string())
            },
        ],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.with_dependencies"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.default")
                )])
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherDependencies".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.with_dependencies")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
    ));
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.with_dependencies")
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
            },
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("with_dependencies".to_string())
            },
        ],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.with_dependencies"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.default")
                )])
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherDependencies".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.with_dependencies")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
    ));
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.with_dependencies")
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![{
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Commit("909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string()),
            }
        }],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from(
                    "WinstonMDP.githubOtherFiles.commit.909896f5646b7fd9f058dcd21961b8d5599dec3b"
                ),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from(
                        "WinstonMDP.githubOtherFiles.commit.909896f5646b7fd9f058dcd21961b8d5599dec3b"
                    )
                )])
            }]
        ]
    );
    assert!(Path::exists(&deps_dir.join(
        "WinstonMDP.githubOtherFiles.commit.909896f5646b7fd9f058dcd21961b8d5599dec3b"
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
            },
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Commit("30cfb86f4e76810eedc1d8d57167289a2b63b4ac".to_string())
            },
        ],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from(
                    "WinstonMDP.githubOtherFiles.commit.30cfb86f4e76810eedc1d8d57167289a2b63b4ac"
                ),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.default")
                )])
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "commit_package".to_string(),
                    OsString::from(
                        "WinstonMDP.githubOtherFiles.commit.30cfb86f4e76810eedc1d8d57167289a2b63b4ac"
                    )
                )])
            }]
        ]
    );
    let commit_dep_dir = deps_dir
        .join("WinstonMDP.githubOtherFiles.commit.30cfb86f4e76810eedc1d8d57167289a2b63b4ac");
    assert!(Path::exists(&commit_dep_dir));
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![LockUnit {
            repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            lock_type: Branch("default".to_string())
        }],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.default")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
    ));
    assert!(!Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.b")
    ));
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![LockUnit {
            repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            lock_type: Branch("b".to_string())
        }],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.b"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.b")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
    ));
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.b")
    ));
    assert_eq!(nfiles(&deps_dir), 2);
}

#[test]
#[allow(clippy::too_many_lines)]
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![LockUnit {
            repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            lock_type: Branch("default".to_string())
        }],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.default")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
            },
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("with_dependencies".to_string())
            },
        ],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.with_dependencies"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.default")
                )])
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherDependencies".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.with_dependencies")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
    ));
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.with_dependencies")
    ));
    assert_eq!(nfiles(&deps_dir), 2);
}

#[test]
fn install_t_9() {
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("cyclic_1".to_string())
            },
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("cyclic_2".to_string())
            },
        ],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![
                BuildUnit {
                    dir: OsString::from("WinstonMDP.githubOtherFiles.branch.cyclic_1"),
                    name_map: BTreeMap::from([(
                        "otherFiles".to_string(),
                        OsString::from("WinstonMDP.githubOtherFiles.branch.cyclic_2")
                    )])
                },
                BuildUnit {
                    dir: OsString::from("WinstonMDP.githubOtherFiles.branch.cyclic_2"),
                    name_map: BTreeMap::from([(
                        "otherFiles".to_string(),
                        OsString::from("WinstonMDP.githubOtherFiles.branch.cyclic_1")
                    )])
                },
            ],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "cycle".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.cyclic_1")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.cyclic_1")
    ));
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.cyclic_2")
    ));
    assert_eq!(nfiles(&deps_dir), 2);
}

#[test]
fn install_t_10() {
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap_err();
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
        version = "1.0.0"
        "#,
    )
    .unwrap();
    let deps_dir = tmp_dir.path().join("deps");
    fs::create_dir(&deps_dir).unwrap();
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![{
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Commit("909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string()),
            }
        }],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from(
                    "WinstonMDP.githubOtherFiles.commit.909896f5646b7fd9f058dcd21961b8d5599dec3b"
                ),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from(
                        "WinstonMDP.githubOtherFiles.commit.909896f5646b7fd9f058dcd21961b8d5599dec3b"
                    )
                )])
            }]
        ]
    );
    assert!(Path::exists(&deps_dir.join(
        "WinstonMDP.githubOtherFiles.commit.909896f5646b7fd9f058dcd21961b8d5599dec3b"
    )));
    assert_eq!(nfiles(&deps_dir), 1);
}

#[test]
fn install_t_12() {
    let tmp_dir = tempfile::tempdir().unwrap();
    fs::write(
        tmp_dir.path().join(CFG_FILE_NAME),
        r#"
        name = "package_name"

        [[deps]]
        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
        version = "=1.0.0"
        "#,
    )
    .unwrap();
    let deps_dir = tmp_dir.path().join("deps");
    fs::create_dir(&deps_dir).unwrap();
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap_err();
}

#[test]
fn install_t_13() {
    let tmp_dir = tempfile::tempdir().unwrap();
    fs::write(
        tmp_dir.path().join(CFG_FILE_NAME),
        r#"
        name = "package_name"

        [[deps]]
        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
        branch = "dev_dep_deps"
        "#,
    )
    .unwrap();
    let deps_dir = tmp_dir.path().join("deps");
    fs::create_dir(&deps_dir).unwrap();
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("dev_dep".to_string())
            },
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("dev_dep_deps".to_string())
            }
        ],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.dev_dep"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.dev_dep_deps"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.dev_dep")
                )])
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.dev_dep_deps")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.dev_dep_deps")
    ));
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.dev_dep")
    ));
    assert_eq!(nfiles(&deps_dir), 2);
}

#[test]
fn install_t_14() {
    let tmp_dir = tempfile::tempdir().unwrap();
    fs::write(
        tmp_dir.path().join(CFG_FILE_NAME),
        r#"
        name = "package_name"

        [[deps]]
        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
        branch = "optional_branch"
        "#,
    )
    .unwrap();
    let deps_dir = tmp_dir.path().join("deps");
    fs::create_dir(&deps_dir).unwrap();
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![LockUnit {
            repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            lock_type: Branch("optional_branch".to_string())
        },],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.optional_branch"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.optional_branch")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.optional_branch")
    ));
    assert!(!Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
    ));
    assert_eq!(nfiles(&deps_dir), 1);
}

#[test]
fn install_t_15() {
    let tmp_dir = tempfile::tempdir().unwrap();
    fs::write(
        tmp_dir.path().join(CFG_FILE_NAME),
        r#"
        name = "package_name"

        [[deps]]
        repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
        branch = "optional_branch"
        options = ["option"]
        "#,
    )
    .unwrap();
    let deps_dir = tmp_dir.path().join("deps");
    fs::create_dir(&deps_dir).unwrap();
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert_eq!(
        lock_file(tmp_dir.path()).unwrap().locks,
        vec![
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
            },
            LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("optional_branch".to_string())
            },
        ],
    );
    assert_eq!(
        build_file(tmp_dir.path()),
        vec![
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.default"),
                name_map: BTreeMap::new()
            }],
            vec![BuildUnit {
                dir: OsString::from("WinstonMDP.githubOtherFiles.branch.optional_branch"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.default")
                )])
            }],
            vec![BuildUnit {
                dir: OsString::from("root"),
                name_map: BTreeMap::from([(
                    "otherFiles".to_string(),
                    OsString::from("WinstonMDP.githubOtherFiles.branch.optional_branch")
                )])
            }]
        ]
    );
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.optional_branch")
    ));
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    fs::write(
        cfg,
        r#"
            name = "package_name"
            "#,
    )
    .unwrap();
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    clean(
        &lock_file(tmp_dir.path()).unwrap().locks,
        &deps_dir,
        &mut empty(),
    )
    .unwrap();
    assert!(!Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
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
    install_cfg(
        tmp_dir.path(),
        &deps_dir,
        &[],
        &stub_installer,
        &mut empty(),
    )
    .unwrap();
    clean(
        &lock_file(tmp_dir.path()).unwrap().locks,
        &deps_dir,
        &mut empty(),
    )
    .unwrap();
    assert!(Path::exists(
        &deps_dir.join("WinstonMDP.githubOtherFiles.branch.default")
    ));
    assert!(!Path::exists(&deps_dir.join("githubOtherFiles.b.branch")));
    assert_eq!(nfiles(&deps_dir), 1);
}
