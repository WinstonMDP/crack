use clap::Parser;
use std::fs;

fn main() {
    let cli = crack::Cli::parse();
    let project_root = crack::project_root().unwrap();
    let dependencies_dir = project_root.join("dependencies");
    let locked_dependencies = crack::locked_dependencies(&project_root);
    match cli.subcommand {
        crack::Subcommand::I => {
            if !dependencies_dir.exists() {
                fs::create_dir_all(&dependencies_dir).unwrap();
            }
            crack::lock(
                &project_root,
                &crack::install(&project_root, &dependencies_dir),
            );
        }
        crack::Subcommand::U => crack::update(&locked_dependencies, &dependencies_dir),
        crack::Subcommand::C => crack::clean(&locked_dependencies, &dependencies_dir),
    };
}
