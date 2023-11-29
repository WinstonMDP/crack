use clap::Parser;
use std::fs;

fn main() {
    let cli = crack::Cli::parse();
    let project_root = crack::project_root().unwrap();
    let dependencies_dir = project_root.join("dependencies");
    let lock_file = project_root.join("crack.lock");
    let locked_dependencies = if lock_file.exists() {
        toml::from_str(&fs::read_to_string(&lock_file).expect("crack.lock should exist"))
            .expect("crack.lock should be valid for deserialisation")
    } else {
        crack::Dependencies::default()
    };
    match cli.subcommand {
        crack::Subcommand::I => {
            if !dependencies_dir.exists() {
                fs::create_dir_all(&dependencies_dir).unwrap();
            }
            let _ = fs::write(
                lock_file,
                toml::to_string(&crack::install(&project_root, &dependencies_dir))
                    .expect("fs::write should fall only in very weird situations"),
            );
        }
        crack::Subcommand::U => crack::update(&locked_dependencies, &dependencies_dir),
        crack::Subcommand::C => crack::clean(&locked_dependencies, &dependencies_dir),
    };
}
