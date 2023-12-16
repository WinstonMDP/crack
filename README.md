# Crack

Specify dependencies in crack.toml.
Example:

```toml
name = "package_name"

[[dependencies.rolling]]
repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
# default branch is git default

[[dependencies.rolling]]
repo = "https://github.com/WinstonMDP/githubOtherFiles.git"
branch = "b"


[[dependencies.commit]]
repo = "git_repository_url"
commit = "sha"
```

Install crack.toml dependencies, which aren't in the directory.
It produces crack.lock.

```zsh
crack i
```

Update crack.lock dependencies.

```zsh
crack u
```

Delete directories, which aren't in the crack.lock.

```zsh
crack c
```
