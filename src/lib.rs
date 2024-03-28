use anyhow::{ensure, Context, Result};
use bimap::BiMap;
use petgraph::{prelude::NodeIndex, Graph};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const CFG_FILE_NAME: &str = "crack.toml";
const LOCK_FILE_NAME: &str = "crack.lock";
pub const BUILD_FILE_NAME: &str = "crack.build";

#[derive(Deserialize, Serialize, Debug)]
pub struct Cfg {
    name: String,
    #[serde(default = "default_interpreter")]
    pub interpreter: PathBuf,
    #[serde(default)]
    dev_deps: Vec<Dep>,
    #[serde(default)]
    deps: Vec<Dep>,
}

fn default_interpreter() -> PathBuf {
    PathBuf::from("/bin/sanskrit")
}

impl Cfg {
    pub fn new(dir: &Path) -> Result<Cfg> {
        let cfg_path = dir.join(CFG_FILE_NAME);
        toml::from_str(
            &fs::read_to_string(&cfg_path)
                .with_context(|| format!("Failed with {cfg_path:#?} cfg file."))?,
        )
        .with_context(|| format!("Failed with {cfg_path:#?} cfg file."))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Dep {
    pub name: Option<String>,
    pub repo: String,
    #[serde(flatten)]
    pub dep_type: Option<DepType>,
    pub options: Option<Vec<String>>,
    pub option_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum DepType {
    Branch(String),
    Commit(String),
    Version(semver::VersionReq),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    pub root_deps: Vec<Dep>,
    pub root_options: HashSet<String>,
    #[serde(default)]
    pub locks: Vec<LockUnit>,
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Clone, Hash)]
pub struct LockUnit {
    repo: String,
    #[serde(flatten)]
    pub lock_type: LockType,
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Clone, Hash)]
#[serde(rename_all = "lowercase")]
pub enum LockType {
    Branch(String),
    Commit(String),
}

/// A unit of a ``BUILD_FILE_NAME`` file.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct BuildUnit {
    dir: OsString,
    name_map: BTreeMap<String, OsString>,
}

/// ``install``, but deps are from the cfg file.
pub fn cfg_install<T: std::io::Write>(
    cfg_dir: &Path,
    deps_dir: &Path,
    options: &HashSet<String>,
    installer: &impl Fn(&Path, &Path, &Path, &OsString, &LockUnit, &mut T) -> Result<()>,
    buffer: &mut T,
) -> Result<()> {
    let cfg = Cfg::new(cfg_dir)?;
    let mut deps = cfg.deps;
    deps.append(&mut cfg.dev_deps.clone());
    install(cfg_dir, deps_dir, deps, options, &installer, buffer)?;
    Ok(())
}

/// Clone branch deps in ``<repo_author>.<repo_name>.branch.<branch>`` dirs and
/// checkout to the respective branch.
/// Clone commit deps in ``<repo_author>.<repo_name>.commit.<commit>`` dirs and
/// checkout to the respective commits.
/// Clone only those repositories, which aren't in ``deps_dir``.
/// Write all deps, which must be contained in ``deps_dir``
/// according to ``deps`` and its transitive deps, to the ``LOCK_FILE_NAME`` file and
/// sccs of deps and root project in reverse topological order to the ``BUILD_FILE_NAME`` file.
pub fn install<T: std::io::Write>(
    cfg_dir: &Path,
    deps_dir: &Path,
    deps: Vec<Dep>,
    options: &HashSet<String>,
    installer: &impl Fn(&Path, &Path, &Path, &OsString, &LockUnit, &mut T) -> Result<()>,
    buffer: &mut T,
) -> Result<()> {
    if !deps_dir.exists() {
        fs::create_dir_all(deps_dir)?;
    }
    let mut graph = Graph::new();
    let mut i_bimap = BiMap::new();
    let cfg_path = cfg_dir.join(CFG_FILE_NAME);
    let mut installed_deps = vec![];
    let mut lock_file = LockFile {
        root_deps: deps.clone(),
        root_options: options.clone(),
        locks: vec![],
    };
    install_h(
        &cfg_path,
        OsString::from("root"),
        deps_dir,
        deps,
        options,
        &installer,
        buffer,
        NodeIndex::from(0),
        &mut i_bimap,
        &mut graph,
        &mut installed_deps,
        &mut HashMap::new(),
    )?;
    let sccs: Vec<Vec<BuildUnit>> = petgraph::algo::kosaraju_scc(&graph)
        .iter()
        .map(|x| {
            x.iter()
                .map(|y| i_bimap.remove_by_right(y).unwrap().0)
                .collect()
        })
        .collect();
    lock_file.locks = installed_deps;
    fs::write(
        cfg_dir.join(LOCK_FILE_NAME),
        toml::to_string(&lock_file).with_context(|| {
            format!("Failed with {:#?} lock file.", cfg_dir.join(LOCK_FILE_NAME))
        })?,
    )?;
    fs::write(
        cfg_dir.join(BUILD_FILE_NAME),
        serde_json::to_string(&sccs)
            .with_context(|| format!("Failed with {BUILD_FILE_NAME} file."))?,
    )?;
    Ok(())
}

fn install_h<T: std::io::Write>(
    cfg_path: &Path,
    cfg_dir: OsString,
    deps_dir: &Path,
    deps: Vec<Dep>,
    options: &HashSet<String>,
    installer: &impl Fn(&Path, &Path, &Path, &OsString, &LockUnit, &mut T) -> Result<()>,
    buffer: &mut T,
    prev_i: NodeIndex,
    i_bimap: &mut BiMap<BuildUnit, NodeIndex>,
    graph: &mut Graph<(), ()>,
    locks: &mut Vec<LockUnit>,
    existing_versions: &mut HashMap<String, Vec<(Version, String)>>,
) -> Result<()> {
    let mut vec_for_name_map = Vec::with_capacity(deps.len());
    let mut vec_to_trans_deps_install = Vec::with_capacity(deps.len());
    for dep in deps {
        if let Some(option_name) = dep.option_name {
            if !options.contains(&option_name) {
                continue;
            }
        }
        let lock = LockUnit {
            lock_type: match dep
                .dep_type
                .unwrap_or(DepType::Branch("default".to_string()))
            {
                DepType::Version(version) => LockType::Commit(
                    existing_versions
                        .entry(dep.repo.clone())
                        .or_insert(version_tags(&dep.repo)?)
                        .iter()
                        .rev()
                        .find(|x| version.matches(&x.0))
                        .with_context(|| format!("There is no {version:?} in {}", dep.repo))?
                        .1
                        .clone(),
                ),
                DepType::Branch(branch) => LockType::Branch(branch),
                DepType::Commit(commit) => LockType::Commit(commit),
            },
            repo: dep.repo,
        };
        let dep_dir_name = dep_dir(&lock)?;
        let dep_dir_path = deps_dir.join(&dep_dir_name);
        installer(
            cfg_path,
            deps_dir,
            &dep_dir_path,
            &dep_dir_name,
            &lock,
            buffer,
        )?;
        let dep_cfg = Cfg::new(&dep_dir_path)?;
        vec_for_name_map.push((dep.name.unwrap_or(dep_cfg.name), dep_dir_name.clone()));
        vec_to_trans_deps_install.push((dep_cfg.deps, dep_dir_name, dep_dir_path, dep.options));
        locks.push(lock);
    }
    let mut name_map = BTreeMap::new();
    for (dep_name, dep_dir) in vec_for_name_map {
        ensure!(
            !name_map.contains_key(&dep_name),
            "Two equal names of deps exist in {cfg_path:?}."
        );
        name_map.insert(dep_name, dep_dir);
    }
    let build_unit = BuildUnit {
        dir: cfg_dir,
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
        for (dep_deps, dep_dir_name, dep_dir_path, options) in vec_to_trans_deps_install {
            install_h(
                &dep_dir_path,
                dep_dir_name,
                deps_dir,
                dep_deps,
                &options.unwrap_or(vec![]).into_iter().collect(),
                installer,
                buffer,
                i,
                i_bimap,
                graph,
                locks,
                existing_versions,
            )?;
        }
    }
    Ok(())
}

/// Return a vec of (version, commit).
fn version_tags(repo: &str) -> Result<Vec<(Version, String)>> {
    let mut v: Vec<(Version, String)> = std::str::from_utf8(
        &Command::new("git")
            .arg("ls-remote")
            .arg("--tags")
            .arg(repo)
            .output()?
            .stdout,
    )?
    .lines()
    .filter_map(|x| {
        let captures = regex::Regex::new(r"(\w*)\s.*v(.*)").ok()?.captures(x)?;
        Some((
            Version::parse(captures.get(2)?.as_str()).ok()?,
            captures.get(1)?.as_str().to_string(),
        ))
    })
    .collect();
    v.sort_unstable_by(|x, y| x.0.cmp(&y.0));
    Ok(v)
}

/// Install deps from remote repos.
pub fn net_installer(
    cfg_path: &Path,
    deps_dir: &Path,
    dep_dir_path: &Path,
    dep_dir_name: &OsString,
    lock: &LockUnit,
    buffer: &mut impl std::io::Write,
) -> Result<()> {
    if !Path::new(dep_dir_path).exists() {
        match lock.lock_type {
            LockType::Commit(ref commit) => {
                std::fs::create_dir(dep_dir_path)?;
                with_stderr_and_context(
                    &Command::new("git")
                        .current_dir(dep_dir_path)
                        .arg("init")
                        .arg("-q")
                        .output()?,
                    cfg_path,
                    lock,
                )?;
                with_stderr_and_context(
                    &Command::new("git")
                        .current_dir(dep_dir_path)
                        .arg("remote")
                        .arg("add")
                        .arg("origin")
                        .arg(&lock.repo)
                        .output()?,
                    cfg_path,
                    lock,
                )?;
                with_stderr_and_context(
                    &Command::new("git")
                        .current_dir(dep_dir_path)
                        .arg("fetch")
                        .arg("-q")
                        .arg("--depth=1")
                        .arg("origin")
                        .arg(commit)
                        .output()?,
                    cfg_path,
                    lock,
                )?;
                with_stderr_and_context(
                    &Command::new("git")
                        .current_dir(dep_dir_path)
                        .arg("checkout")
                        .arg("-q")
                        .arg("FETCH_HEAD")
                        .output()?,
                    cfg_path,
                    lock,
                )?;
                writeln!(buffer, "{lock:?} was installed.")?;
            }
            LockType::Branch(ref branch) => {
                let mut command = Command::new("git");
                let command = command
                    .current_dir(deps_dir)
                    .arg("clone")
                    .arg("-q")
                    .arg("--depth=1")
                    .arg(&lock.repo);
                if branch != "default" {
                    command.arg("-b").arg(branch);
                }
                with_stderr_and_context(&command.arg(dep_dir_name).output()?, cfg_path, lock)?;
                writeln!(buffer, "{lock:?} was installed.")?;
            }
        }
    };
    Ok(())
}

fn with_stderr_and_context(
    output: &std::process::Output,
    cfg: &Path,
    lock: &LockUnit,
) -> Result<()> {
    with_sterr(output).with_context(|| format!("Failed with {lock:?} in {cfg:?} cfg file."))
}

pub fn with_sterr(output: &std::process::Output) -> Result<()> {
    ensure!(
        output.stderr.is_empty(),
        std::str::from_utf8(&output.stderr)?.to_string()
    );
    Ok(())
}

/// Delete deps dirs, which aren't in the ``LOCK_FILE_NAME`` file.
pub fn clean(locks: &[LockUnit], deps_dir: &Path, buffer: &mut impl std::io::Write) -> Result<()> {
    let locked_dep_dirs = locks
        .iter()
        .map(dep_dir)
        .collect::<Result<HashSet<OsString>>>()?;
    for file in fs::read_dir(deps_dir)? {
        let dir = file
            .with_context(|| format!("Failed with {deps_dir:#?} deps dir."))?
            .file_name();
        if !locked_dep_dirs.contains(&dir) {
            fs::remove_dir_all(deps_dir.join(&dir))
                .with_context(|| format!("Failed with {dir:#?} dir."))?;
            writeln!(buffer, "{dir:#?} was deleted.")?;
        }
    }
    Ok(())
}

pub fn dep_dir(lock: &LockUnit) -> Result<OsString> {
    match &lock.lock_type {
        LockType::Commit(commit) => {
            let mut dir = OsString::from(repo_author_and_name(&lock.repo)?);
            dir.push(".commit");
            dir.push(".");
            dir.push(commit);
            Ok(dir)
        }
        LockType::Branch(branch) => {
            let mut dir = OsString::from(repo_author_and_name(&lock.repo)?);
            dir.push(".branch");
            dir.push(".");
            dir.push(&branch.clone());
            Ok(dir)
        }
    }
}

fn repo_author_and_name(git_url: &str) -> Result<String> {
    let captures = regex::Regex::new(r"([\w-]*)\/([\w-]*)\.git")?
        .captures(git_url)
        .with_context(|| {
            format!(
                r#"Can't capture repository name in "{git_url}". Probably, ".git" part is missed."#
            )
        })?;
    Ok(captures.get(1).with_context(|| format!(r#"Can't capture author name in "{git_url}"."#))?.as_str().to_string()
       +
       "."
       +
       captures.get(2).with_context(|| format!(r#"Can't capture repository name in "{git_url}". Probably, ".git" part is missed."#))?.as_str()
    )
}

/// Content of the ``LOCK_FILE_NAME`` file.
pub fn lock_file(lock_file_dir: &Path) -> Result<LockFile> {
    let lock_file = lock_file_dir.join(LOCK_FILE_NAME);
    Ok(if lock_file.exists() {
        toml::from_str(
            &fs::read_to_string(&lock_file)
                .with_context(|| format!("Failed with {lock_file:#?} lock file."))?,
        )
        .with_context(|| format!("Failed with {lock_file:#?} lock file."))?
    } else {
        LockFile {
            root_deps: vec![],
            root_options: HashSet::new(),
            locks: vec![],
        }
    })
}

#[cfg(test)]
mod tests;
