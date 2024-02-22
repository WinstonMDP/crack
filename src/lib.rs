use anyhow::{anyhow, ensure, Context, Result};
use bimap::BiMap;
use petgraph::{prelude::NodeIndex, Graph};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    fs,
    path::Path,
    process::Command,
};

pub const CFG_FILE_NAME: &str = "crack.toml";
const LOCK_FILE_NAME: &str = "crack.lock";

#[derive(Deserialize, Serialize, Debug)]
pub struct Cfg {
    pub name: String,
    #[serde(default)]
    pub dev_deps: Vec<Dep>,
    #[serde(default)]
    pub deps: Vec<Dep>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Dep {
    pub name: Option<String>,
    pub repo: String,
    #[serde(flatten)]
    pub dep_type: Option<DepType>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DepType {
    Branch(String),
    Commit(String),
    Version(semver::VersionReq),
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Hash, Clone)]
pub struct LockUnit {
    repo: String,
    #[serde(flatten)]
    pub lock_type: LockType,
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Hash, Clone)]
#[serde(rename_all = "lowercase")]
pub enum LockType {
    Branch(String),
    Commit(String),
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct BuildUnit {
    dir: OsString,
    name_map: BTreeMap<String, OsString>,
}

/// Clone branch deps in ``<repo_author>.<repo_name>.branch.<branch>`` dirs and
/// checkout to the respective branch.
/// Clone commit deps in ``<repo_author>.<repo_name>.commit.<commit>`` dirs and
/// checkout to the respective commits.
/// Clone only those repositories, which aren't in ``deps_dir``.
/// Returns all deps, which must be contained in ``deps_dir``
/// according to cfg and its transitive deps, and sccs of deps and root
/// project in reverse topological order.
pub fn install<T: std::io::Write>(
    cfg_dir: &Path,
    deps_dir: &Path,
    installer: &impl Fn(&Path, &Path, &Path, &OsString, &LockUnit, &mut T) -> Result<()>,
    buffer: &mut T,
) -> Result<(Vec<LockUnit>, Vec<Vec<BuildUnit>>)> {
    let mut graph: Graph<(), ()> = Graph::new();
    let mut i_bimap = BiMap::new();
    let cfg_path = cfg_dir.join(CFG_FILE_NAME);
    let mut installed_deps = vec![];
    let mut cfg = Cfg::from(&cfg_path)?;
    let mut deps_to_install = cfg.deps;
    deps_to_install.append(&mut cfg.dev_deps);
    install_h(
        &cfg_path,
        OsString::from("root"),
        deps_dir,
        deps_to_install,
        &installer,
        buffer,
        NodeIndex::from(0),
        &mut i_bimap,
        &mut graph,
        &mut installed_deps,
        &mut HashMap::new(),
    )?;
    let sccs = petgraph::algo::kosaraju_scc(&graph)
        .iter()
        .map(|x| {
            x.iter()
                .map(|y| i_bimap.remove_by_right(y).unwrap().0)
                .collect()
        })
        .collect();
    installed_deps.sort_unstable();
    installed_deps.dedup();
    Ok((installed_deps, sccs))
}

fn install_h<T: std::io::Write>(
    cfg_path: &Path,
    cfg_dir: OsString,
    deps_dir: &Path,
    deps: Vec<Dep>,
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
        let lock = LockUnit {
            lock_type: match dep
                .dep_type
                .unwrap_or(DepType::Branch("default".to_string()))
            {
                DepType::Version(version) => LockType::Commit(
                    existing_versions
                        .entry(dep.repo.clone())
                        .or_insert(tags(&dep.repo)?)
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
        let dep_cfg = Cfg::from(&dep_dir_path.join(CFG_FILE_NAME))?;
        vec_for_name_map.push((dep.name.unwrap_or(dep_cfg.name), dep_dir_name.clone()));
        vec_to_trans_deps_install.push((dep_cfg.deps, dep_dir_name, dep_dir_path));
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
        for (dep_deps, dep_dir_name, dep_dir_path) in vec_to_trans_deps_install {
            install_h(
                &dep_dir_path,
                dep_dir_name,
                deps_dir,
                dep_deps,
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

fn tags(repo: &str) -> Result<Vec<(Version, String)>> {
    Ok(std::str::from_utf8(
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
    .collect())
}

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
                writeln!(buffer, "{lock:?} was installed")?;
            }
            LockType::Branch(ref branch) => {
                let mut command = Command::new("git");
                let mut command = command
                    .current_dir(deps_dir)
                    .arg("clone")
                    .arg("-q")
                    .arg("--depth=1")
                    .arg(&lock.repo);
                if branch != "default" {
                    command = command.arg("-b").arg(branch);
                }
                with_stderr_and_context(&command.arg(dep_dir_name).output()?, cfg_path, lock)?;
                writeln!(buffer, "{lock:?} was installed")?;
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
    with_sterr(output).with_context(|| format!("Failed with {cfg:?} cfg file, {lock:?}."))
}

pub fn with_sterr(output: &std::process::Output) -> Result<()> {
    ensure!(
        output.stderr.is_empty(),
        std::str::from_utf8(&output.stderr)?.to_string()
    );
    Ok(())
}

fn is_sorted<T: Ord + Clone>(array: &[T]) -> bool {
    let mut cloned = array.to_vec();
    cloned.sort();
    cloned == array
}

/// Delete deps dirs, which aren't in ``LOCK_FILE_NAME`` file.
pub fn clean(locks: &[LockUnit], deps_dir: &Path, buffer: &mut impl std::io::Write) -> Result<()> {
    debug_assert!(is_sorted(locks));
    let locked_dep_dirs = locks
        .iter()
        .map(dep_dir)
        .collect::<Result<Vec<OsString>>>()?;
    for file in fs::read_dir(deps_dir)? {
        let dir = file
            .with_context(|| format!("Failed with {deps_dir:#?} deps dir."))?
            .file_name();
        if locked_dep_dirs.binary_search(&dir).is_err() {
            fs::remove_dir_all(deps_dir.join(&dir))
                .with_context(|| format!("Failed with {dir:#?} dir."))?;
            writeln!(buffer, "{dir:#?} was deleted")?;
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
        .ok_or_else(|| { anyhow!("Can't capture repository name in \"{git_url}\". Probably, \".git\" part is missed.") })?;
    Ok(captures.get(1).ok_or_else(|| { anyhow!("Can't capture author name in \"{git_url}\".") })? .as_str().to_string()
        +
        "."
        +
        captures.get(2).ok_or_else(|| { anyhow!("Can't capture repository name in \"{git_url}\". Probably, \".git\" part is missed.") })?.as_str()
    )
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
        Ok(vec![])
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
mod tests;
