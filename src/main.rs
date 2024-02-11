use anyhow::{Context, Result};
use clap::Parser;
use std::{fs, path::Path};

#[derive(clap::Parser)]
#[command(about = "A Sanskrit package manager", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Subcommand,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Install crack.toml deps, which aren't in the deps directory.
    I,
    /// Update crack.lock deps.
    U,
    /// Delete directories, which aren't in crack.lock.
    C,
}

fn install(project_root: &Path, deps_dir: &Path) -> Result<()> {
    if !deps_dir.exists() {
        fs::create_dir_all(deps_dir)?;
    }
    let installed = crack::install(project_root, deps_dir, &mut std::io::stdout())?;
    crack::lock(project_root, &installed.0)?;
    fs::write(
        "crack.build",
        serde_json::to_string(&installed.1).context("Failed with crack.build file.")?,
    )?;
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let project_root = std::env::current_dir()?
        .ancestors()
        .find(|x| x.join(crack::CFG_FILE_NAME).exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Can't find {} in the current and ancestor directories.",
                crack::CFG_FILE_NAME
            )
        })?;
    let deps_dir = project_root.join("deps");
    match cli.subcommand {
        Subcommand::I => install(&project_root, &deps_dir)?,
        Subcommand::U => {
            let locked_deps = crack::locked_deps(&project_root)?;
            for lock in &locked_deps {
                if let crack::LockUnit::Rolling { .. } = lock {
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
                }
            }
            install(&project_root, &deps_dir)?;
        }
        Subcommand::C => {
            if deps_dir.exists() {
                let locked_deps = crack::locked_deps(&project_root)?;
                fs::create_dir_all(&deps_dir)?;
                crack::clean(&locked_deps, &deps_dir, &mut std::io::stdout())?;
            } else {
                println!("There is nothing to clean. {deps_dir:#?} directory doesn't exist.");
            }
        }
    };
    Ok(())
}
