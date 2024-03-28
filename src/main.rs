use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use crack::with_sterr;
use std::{
    collections::HashMap,
    fs,
    io::{stdout, Write},
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
    /// Install crack.toml deps, which aren't in the deps directory, and produce crack.build.
    #[clap(visible_alias = "i")]
    Install { options: Option<Vec<String>> },
    /// Update deps, which are in crack.lock.
    #[clap(visible_alias = "u")]
    Update,
    /// Update the registry.
    #[clap(visible_alias = "ur")]
    UpdateRegistry,
    /// Delete directories, which aren't in crack.lock.
    #[clap(visible_alias = "c")]
    Clean,
    /// Create an empty project
    #[clap(visible_alias = "n")]
    New { project_name: std::ffi::OsString },
    /// Build the project
    #[clap(visible_alias = "b")]
    Build {
        #[clap(short, long)]
        interpreter: Option<PathBuf>,
        #[clap(short, long)]
        build_file: Option<PathBuf>,
    },
    /// Run the project program
    #[clap(visible_alias = "r")]
    Run {
        #[clap(short, long)]
        interpreter: Option<PathBuf>,
        #[clap(short, long)]
        build_file: Option<PathBuf>,
    },
    /// Add a dep to crack.toml.
    #[clap(visible_alias = "a")]
    Add { dep_name: String },
    /// Add a dev-dep to crack.toml.
    #[clap(visible_alias = "ad")]
    AddDev { dev_dep_name: String },
    /// Generate completion
    Completion { shell: clap_complete::Shell },
}

fn registry() -> Result<HashMap<String, String>> {
    Ok(toml::from_str(&fs::read_to_string(
        Path::new(&std::env::var("HOME")?)
            .join(".crack")
            .join("registry.toml"),
    )?)?)
}

fn project_root() -> Result<PathBuf> {
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
            (format!("\n[[{}deps]]\n", if dev_deps {"dev_"} else {""}) // hardcode [[deps]]
                + &toml::to_string(&crack::Dep {
                    name: None,
                    repo: registry()?
                    .remove(dep_name)
                    .with_context(|| format!("There is no {dep_name} in the registry."))?,
                    dep_type: None,
                    options: None,
                    option_name: None
                })?)
                .as_bytes(),
        )?;
    Ok(())
}

enum BuildOrRun {
    Build,
    Run,
}

fn build_or_run(
    build_or_run: &BuildOrRun,
    interpreter: Option<PathBuf>,
    build_file: Option<PathBuf>,
) -> Result<()> {
    let project_root = project_root()?;
    let interpreter = interpreter.unwrap_or(crack::Cfg::new(&project_root)?.interpreter);
    anyhow::ensure!(
        interpreter.is_absolute(),
        "The interpreter path must be absolute."
    );
    let mut command = Command::new(interpreter);
    command.current_dir(&project_root);
    if let BuildOrRun::Build = build_or_run {
        command.arg("--check");
    }
    let output = command
        .arg(build_file.unwrap_or(project_root.join(crack::BUILD_FILE_NAME).canonicalize()?))
        .output()?;
    with_sterr(&output)?;
    println!("{}", std::str::from_utf8(&output.stdout)?);
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.subcommand {
        Subcommand::Install { options } => {
            let project_root = project_root()?;
            let deps_dir = project_root.join("deps");
            crack::cfg_install(
                &project_root,
                &deps_dir,
                &options.unwrap_or(vec![]).into_iter().collect(),
                &crack::net_installer,
                &mut stdout(),
            )?;
        }
        Subcommand::Update => {
            let project_root = project_root()?;
            let deps_dir = project_root.join("deps");
            let lock_file = crack::lock_file(&project_root)?;
            for lock in &lock_file.locks {
                if let crack::LockType::Branch(..) = lock.lock_type {
                    let dir = deps_dir.join(crack::dep_dir(lock)?);
                    crack::with_sterr(
                        &Command::new("git")
                            .current_dir(&dir)
                            .arg("pull")
                            .arg("-q")
                            .arg("--depth=1")
                            .output()?,
                    )
                    .with_context(|| format!("Failed with {dir:#?} directory."))?;
                    println!("{lock:?} was processed.");
                }
            }
            crack::install(
                &project_root,
                &deps_dir,
                lock_file.root_deps,
                &lock_file.root_options,
                &crack::net_installer,
                &mut stdout(),
            )?;
        }
        Subcommand::Clean => {
            let project_root = project_root()?;
            let deps_dir = project_root.join("deps");
            if deps_dir.exists() {
                let lock_file = crack::lock_file(&project_root)?;
                fs::create_dir_all(&deps_dir)?;
                crack::clean(&lock_file.locks, &deps_dir, &mut std::io::stdout())?;
            } else {
                println!("There is nothing to clean. {deps_dir:#?} directory doesn't exist.");
            }
        }
        Subcommand::New { project_name } => {
            let project_dir = std::env::current_dir()?.join(&project_name);
            fs::create_dir(&project_dir)?;
            fs::write(
                project_dir.join(crack::CFG_FILE_NAME),
                format!("name = {project_name:?}"),
            )?;
        }
        Subcommand::Build {
            interpreter,
            build_file,
        } => build_or_run(&BuildOrRun::Build, interpreter, build_file)?,
        Subcommand::Run {
            interpreter,
            build_file,
        } => build_or_run(&BuildOrRun::Run, interpreter, build_file)?,
        Subcommand::Add { dep_name } => add(&dep_name, false)?,
        Subcommand::AddDev { dev_dep_name } => add(&dev_dep_name, true)?,
        Subcommand::UpdateRegistry => {
            let registry_dir = Path::new(&std::env::var("HOME")?).join(".crack");
            if !registry_dir.exists() {
                fs::create_dir(&registry_dir)?;
            }
            fs::write(
                registry_dir.join("registry.toml"),
                reqwest::blocking::get(
                    "https://gist.githubusercontent.com/WinstonMDP/2a7e34b7c9a514d20d13a41773b3defc/raw/d9b529ca18d2be2a00270dc3f1c141e7ecbf82c2/registry.toml"
                )?
                .text()?,
            )?;
        }
        Subcommand::Completion { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "crack", &mut stdout());
        }
    };
    Ok(())
}
