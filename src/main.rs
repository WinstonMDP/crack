use clap::Parser;

fn main() {
    let cli = crack::Cli::parse();
    match cli.subcommand {
        crack::Subcommand::I => crack::install(),
        crack::Subcommand::U => crack::update(),
        crack::Subcommand::C => crack::clean(),
    };
}
