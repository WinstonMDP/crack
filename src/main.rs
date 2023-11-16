use crack::clone_commit_dependencies;

fn main() {
    clone_commit_dependencies(&[(
        "https://github.com/WinstonMDP/WinstonMDP.git",
        "97f65b91f9748ad6141774905ce224e2cc5469f2",
    )]);
}
