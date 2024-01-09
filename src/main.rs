use anyhow::Context;
use clap::Parser;
use std::{collections::HashMap, fs, io::Write};

#[derive(clap::Parser)]
#[command(about = "A Sanskrit package manager", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Subcommand,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Install crack.toml dependencies, which aren't in the dependencies directory.
    I,
    ///  Add a rolling dependency to crack.toml, if dependency exists in the repos table.
    A { dependency: String },
    /// Update crack.lock dependencies.
    U,
    /// Delete directories, which aren't in crack.lock.
    C,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let project_root = std::env::current_dir()?
        .ancestors()
        .find(|x| x.join(crack::CFG_FILE_NAME).exists())
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Can't find {} in the current and ancestor directories.",
                crack::CFG_FILE_NAME
            )
        })?;
    let dependencies_dir = project_root.join("dependencies");
    match cli.subcommand {
        Subcommand::I => {
            if !dependencies_dir.exists() {
                fs::create_dir_all(&dependencies_dir)?;
            }
            crack::lock(
                &project_root,
                &crack::install(&project_root, &dependencies_dir, &mut std::io::stdout())?,
            )?;
        }
        Subcommand::A { dependency } => {
            let repos_table: std::collections::HashMap<String, crack::RollingDependency> =
                toml::from_str(
                    &fs::read_to_string("repos_table.toml")
                        .context("Failed with the repos table.")?,
                )
                .context("Failed with the repos table.")?;
            match repos_table.get(&dependency) {
                Some(dependency) => {
                    fs::OpenOptions::new()
                        .append(true)
                        .open(crack::CFG_FILE_NAME)
                        .context("Failed with the cfg file.")?
                        .write_all(
                            // NOTE: perfomance will be better, if a more cheap-const version of map exists
                            ("\n".to_string()
                                + &toml::to_string(&HashMap::from([(
                                    "dependencies",
                                    HashMap::from([("rolling", vec![dependency])]),
                                )]))
                                .unwrap())
                                .as_bytes(),
                        )
                        .context("Failed with the cfg file.")?;
                    println!("{dependency:?} was added.");
                }
                None => println!("No such dependency in the repos table"),
            }
        }
        Subcommand::U => {
            let locked_dependencies = crack::locked_dependencies(&project_root)?;
            for dependency in &locked_dependencies.rolling {
                let dir = dependencies_dir.join(crack::rolling_dependency_dir(dependency)?);
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
        Subcommand::C => {
            if dependencies_dir.exists() {
                let locked_dependencies = crack::locked_dependencies(&project_root)?;
                fs::create_dir_all(&dependencies_dir)?;
                crack::clean(
                    &locked_dependencies,
                    &dependencies_dir,
                    &mut std::io::stdout(),
                )?;
            } else {
                println!(
                    "There is nothing to clean. {dependencies_dir:#?} directory doesn't exist."
                );
            }
        }
    };
    Ok(())
}
