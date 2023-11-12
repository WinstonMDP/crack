use crack::clone_rolling_dependencies;

fn main() {
    // let string_config = r#"
    //     name = "package_name"
    //     dependencies.rolling = [
    //         "https://github.com/monkeyjunglejuice/matrix-emacs-theme",
    //         "https://github.com/agda/agda"
    //     ]
    //
    //     [[dependencies.commit]]
    //     repository = "https://github.com/jesseleite/nvim-noirbuddy"
    //     commit = "7d92fc64ae4c23213fd06f0464a72de45887b0ba"
    //     "#;
    // let config: toml::Table = toml::from_str(string_config).unwrap();
    // println!("{}", config);
    clone_rolling_dependencies(&["https://github.com/WinstonMDP/githubOtherFiles"]);
}
