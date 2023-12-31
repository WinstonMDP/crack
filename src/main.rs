use anyhow::Context;
use clap::Parser;
use crack::Subcommand;
use std::fs;

fn main() -> anyhow::Result<()> {
    let cli = crack::Cli::parse();
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
                fs::create_dir_all(&dependencies_dir).unwrap();
            }
            crack::lock(
                &project_root,
                &crack::install(&project_root, &dependencies_dir, &mut std::io::stdout())?,
            )?;
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
                fs::create_dir_all(&dependencies_dir).unwrap();
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
