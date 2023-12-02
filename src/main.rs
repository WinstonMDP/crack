use clap::Parser;
use std::fs;

fn main() -> anyhow::Result<()> {
    let cli = crack::Cli::parse();
    let project_root = crack::project_root()?;
    let dependencies_dir = project_root.join("dependencies");
    let locked_dependencies = crack::locked_dependencies(&project_root);
    match cli.subcommand {
        crack::Subcommand::I => {
            if !dependencies_dir.exists() {
                fs::create_dir_all(&dependencies_dir).unwrap();
            }
            crack::lock(
                &project_root,
                &crack::install(&project_root, &dependencies_dir)?,
            )?;
        }
        crack::Subcommand::U => {
            if dependencies_dir.exists() {
                let locked_dependencies = locked_dependencies?;
                for dependency in &locked_dependencies.rolling {
                    let _ = std::process::Command::new("git")
                        .current_dir(
                            dependencies_dir.join(crack::rolling_dependency_dir(dependency)),
                        )
                        .arg("pull")
                        .output();
                }
            }
        }
        crack::Subcommand::C => {
            if dependencies_dir.exists() {
                let locked_dependencies = locked_dependencies?;
                fs::create_dir_all(&dependencies_dir).unwrap();
                crack::clean(&locked_dependencies, &dependencies_dir)?;
            }
        }
    };
    Ok(())
}
