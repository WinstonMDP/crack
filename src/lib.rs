#![warn(clippy::pedantic)]

pub fn clone_rolling_dependencies(repositories: &[&str]) {
    for repository in repositories {
        let _ = std::process::Command::new("git")
            .current_dir("tests/dependencies")
            .arg("clone")
            .arg(repository)
            .arg(repository_name(repository))
            .output();
    }
}

/// ``git_url`` must consists ".git" part
fn repository_name(git_url: &str) -> String {
    todo!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_name_t_1() {
        assert_eq!(
            repository_name("https://github.com/WinstonMDP/repo_name.git"),
            "repo_name"
        );
    }
}
