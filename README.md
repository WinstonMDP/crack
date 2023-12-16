# Crack

Specify dependencies in ``crack.toml``. ``crack.toml`` must be in the project root.

``crack.toml`` example:

```toml
name = "package_name"

[[dependencies.rolling]]
repo = "git_repo_url"
# default branch is git default

[[dependencies.rolling]]
repo = "git_repo_url"
branch = "git_rep_branch"


[[dependencies.commit]]
repo = "git_repository_url"
commit = "sha"
```

All dependencies are stored in ``project_root/dependencies`` directory.

## Commands

Install ``crack.toml`` dependencies, which aren't in dependencies directory.
It produces ``crack.lock`` and dependencies directory, if it doesn't exist.

```zsh
crack i
```

Update ``crack.lock`` dependencies.

```zsh
crack u
```

Delete directories, which aren't in ``crack.lock``.

```zsh
crack c
```
