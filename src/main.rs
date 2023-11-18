fn main() {
    let config: crack::Config = toml::from_str(
        r#"
name = "package_name"
dependencies.rolling = [
    "https://github.com/monkeyjunglejuice/matrix-emacs-theme.git",
    "https://github.com/agda/agda.git"
]

[[dependencies.commit]]
repository = "https://github.com/jesseleite/nvim-noirbuddy"
commit = "7d92fc64ae4c23213fd06f0464a72de45887b0ba"
                                         "#,
    )
    .unwrap();
    println!("{:#?}", config);
    println!("{:#?}", toml::to_string(&config));
}
