use anyhow::Context;
use clap::Parser;
use std::fs;

fn main() -> anyhow::Result<()> {
    let cli = crack::Cli::parse();
    let project_root = crack::project_root()?;
    let dependencies_dir = project_root.join("dependencies");
    match cli.subcommand {
        crack::Subcommand::I => {
            if !dependencies_dir.exists() {
                fs::create_dir_all(&dependencies_dir).unwrap();
            }
            crack::lock(
                &project_root,
                &crack::install(&project_root, &dependencies_dir, &mut std::io::stdout())?,
            )?;
        }
        crack::Subcommand::U => {
            let locked_dependencies = crack::locked_dependencies(&project_root)?;
            for dependency in &locked_dependencies.rolling {
                let dir = dependencies_dir.join(crack::rolling_dependency_dir(dependency)?);
                std::process::Command::new("git")
                    .current_dir(&dir)
                    .arg("pull")
                    .output()
                    .with_context(|| format!("Failed with directory {dir:#?}."))?;
            }
        }
        crack::Subcommand::C => {
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
                    "There is nothing to clean. Directory {dependencies_dir:#?} doesn't exist."
                );
            }
        }
    };
    Ok(())
}
