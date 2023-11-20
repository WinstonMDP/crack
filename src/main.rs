use clap::Parser;

fn main() {
    let cli = crack::Cli::parse();
    match cli.command {
        crack::Commands::Install => println!("Install"),
        crack::Commands::Update => println!("Update"),
        crack::Commands::Clean => println!("Clean"),
    };
}
