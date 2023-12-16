# Crack

Specify dependencies in ``crack.toml``.
``crack.toml`` must be in the project root.
Example:

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

Install ``crack.toml`` dependencies, which aren't in the directory.
It produces ``crack.lock``.

```zsh
crack i
```

Update ``crack.lock`` dependencies.

```zsh
crack u
```

Delete directories, which aren't in the ``crack.lock``.

```zsh
crack c
```
