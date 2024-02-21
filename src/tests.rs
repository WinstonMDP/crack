use super::*;
use std::{fs, io::empty, path::Path};
use LockType::{Branch, Commit};

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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
            vec![LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
            vec![LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
            vec![LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("main".to_string())
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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
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
    assert_eq!(
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
            vec![{
                LockUnit {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    lock_type: Commit("909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string()),
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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
            vec![LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
            vec![LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("b".to_string())
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
    assert_eq!(
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
            vec![LockUnit {
                repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                lock_type: Branch("default".to_string())
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
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
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
    assert_eq!(
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
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
    install(
        tmp_dir.path(),
        &deps_dir,
        &HashMap::new(),
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
    assert_eq!(
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::from([(
                "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                vec![
                    (Version::new(0, 3, 5), "asdkl;j;k".to_string()),
                    (
                        Version::new(1, 3, 5),
                        "909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string()
                    ),
                    (Version::new(2, 0, 1), "2343128908".to_string())
                ]
            )]),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
            vec![{
                LockUnit {
                    repo: "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
                    lock_type: Commit("909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string()),
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
fn install_t_12() {
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
    install(
        tmp_dir.path(),
        &deps_dir,
        &HashMap::from([(
            "https://github.com/WinstonMDP/githubOtherFiles.git".to_string(),
            vec![
                (
                    Version::new(0, 3, 5),
                    "909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string(),
                ),
                (
                    Version::new(2, 0, 1),
                    "909896f5646b7fd9f058dcd21961b8d5599dec3b".to_string(),
                ),
            ],
        )]),
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
    assert_eq!(
        install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap(),
        (
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
            vec![
                vec![BuildUnit {
                    dir: OsString::from("githubOtherFiles.rolling.dev_dep"),
                    name_map: BTreeMap::new()
                }],
                vec![BuildUnit {
                    dir: OsString::from("githubOtherFiles.rolling.dev_dep_deps"),
                    name_map: BTreeMap::from([(
                        "otherFiles".to_string(),
                        OsString::from("githubOtherFiles.rolling.dev_dep")
                    )])
                }],
                vec![BuildUnit {
                    dir: OsString::from("root"),
                    name_map: BTreeMap::from([(
                        "otherFiles".to_string(),
                        OsString::from("githubOtherFiles.rolling.dev_dep_deps")
                    )])
                }]
            ]
        )
    );
    assert!(Path::exists(
        &deps_dir.join("githubOtherFiles.rolling.dev_dep_deps")
    ));
    assert!(Path::exists(
        &deps_dir.join("githubOtherFiles.rolling.dev_dep")
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
        &install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap()
        .0,
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
        &install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap()
        .0,
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
        &install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap()
        .0,
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
        &install(
            tmp_dir.path(),
            &deps_dir,
            &HashMap::new(),
            &stub_installer,
            &mut empty(),
        )
        .unwrap()
        .0,
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
