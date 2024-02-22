use anyhow::{Context, Result};
use clap::Parser;
use std::{collections::HashMap, fs, io::Write, path::Path};

#[derive(clap::Parser)]
#[command(about = "A Sanskrit package manager", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Subcommand,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Install crack.toml deps, which aren't in the deps directory, and produce crack.build.
    I,
    /// Install new deps and update all deps.
    U,
    /// Update registry.
    Ur,
    /// Delete directories, which aren't in crack.lock.
    C,
    /// Create empty project
    N { project_name: std::ffi::OsString },
    /// Build project
    B,
    /// Run project program
    R,
    /// Add dep to crack.toml.
    A { dep_name: String },
    /// Add dev-dep to crack.toml.
    Ad { dev_dep_name: String },
}

fn registry() -> Result<HashMap<String, String>> {
    Ok(toml::from_str(&fs::read_to_string(
        Path::new(&std::env::var("HOME")?)
            .join(".crack")
            .join("names.toml"),
    )?)?)
}

fn install(project_root: &Path, deps_dir: &Path) -> Result<()> {
    if !deps_dir.exists() {
        fs::create_dir_all(deps_dir)?;
    }
    let installed = crack::install(
        project_root,
        deps_dir,
        &crack::net_installer,
        &mut std::io::stdout(),
    )?;
    crack::lock(project_root, &installed.0)?;
    fs::write(
        "crack.build",
        serde_json::to_string(&installed.1).context("Failed with crack.build file.")?,
    )?;
    Ok(())
}

fn project_root() -> Result<std::path::PathBuf> {
    std::env::current_dir()?
        .ancestors()
        .find(|x| x.join(crack::CFG_FILE_NAME).exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Can't find {} in the current and ancestor directories.",
                crack::CFG_FILE_NAME
            )
        })
}

fn add(dep_name: &str, dev_deps: bool) -> Result<()> {
    fs::OpenOptions::new()
        .append(true)
        .open(project_root()?.join(crack::CFG_FILE_NAME))?
        .write_all(
            (format!("\n[[{}deps]]\n", if dev_deps {"dev_"} else {""}) // TODO: hardcode [[deps]]
                + &toml::to_string(&crack::Dep {
                    name: None,
                    repo: registry()?
                    .remove(dep_name)
                    .with_context(|| format!("There is no {dep_name} in the registry."))?,
                    dep_type: None,
                })?)
                .as_bytes(),
        )?;
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.subcommand {
        Subcommand::I => {
            let project_root = project_root()?;
            let deps_dir = project_root.join("deps");
            install(&project_root, &deps_dir)?;
        }
        Subcommand::U => {
            let project_root = project_root()?;
            let deps_dir = project_root.join("deps");
            install(&project_root, &deps_dir)?;
            let locked_deps = crack::locked_deps(&project_root)?;
            for lock in &locked_deps {
                if let crack::LockType::Branch(..) = lock.lock_type {
                    let dir = deps_dir.join(crack::dep_dir(lock)?);
                    crack::with_sterr(
                        &std::process::Command::new("git")
                            .current_dir(&dir)
                            .arg("pull")
                            .arg("-q")
                            .arg("--depth=1")
                            .output()?,
                    )
                    .with_context(|| format!("Failed with {dir:#?} directory."))?;
                    println!("{lock:?} was processed");
                }
            }
            install(&project_root, &deps_dir)?;
        }
        Subcommand::C => {
            let project_root = project_root()?;
            let deps_dir = project_root.join("deps");
            if deps_dir.exists() {
                let locked_deps = crack::locked_deps(&project_root)?;
                fs::create_dir_all(&deps_dir)?;
                crack::clean(&locked_deps, &deps_dir, &mut std::io::stdout())?;
            } else {
                println!("There is nothing to clean. {deps_dir:#?} directory doesn't exist.");
            }
        }
        Subcommand::N { project_name } => {
            let project_dir = std::env::current_dir()?.join(&project_name);
            fs::create_dir(&project_dir)?;
            fs::write(
                project_dir.join(crack::CFG_FILE_NAME),
                format!("name = {project_name:?}"),
            )?;
        }
        Subcommand::B => todo!(),
        Subcommand::R => todo!(),
        Subcommand::A { dep_name } => add(&dep_name, false)?,
        Subcommand::Ad { dev_dep_name } => add(&dev_dep_name, true)?,
        Subcommand::Ur => {
            let registry_dir = Path::new(&std::env::var("HOME")?).join(".crack");
            if !registry_dir.exists() {
                fs::create_dir(&registry_dir)?;
            }
            fs::write(
                registry_dir.join("names.toml"),
                reqwest::blocking::get(
                    "https://gist.githubusercontent.com/WinstonMDP/2a7e34b7c9a514d20d13a41773b3defc/raw/d9b529ca18d2be2a00270dc3f1c141e7ecbf82c2/registry.toml"
                )?
                .text()?,
            )?;
        }
    };
    Ok(())
}
