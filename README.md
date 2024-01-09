# Crack

Specify dependencies in ``crack.toml``. ``crack.toml`` must be in the project root.

``crack.toml`` example:

```toml
name = "package_name"

[[dependencies.rolling]]
repo = "git_repo_url"
# a default branch is git default

[[dependencies.rolling]]
repo = "git_repo_url"
branch = "git_repo_branch"


[[dependencies.commit]]
repo = "git_repo_url"
commit = "sha"
```

All dependencies are stored in ``project_root/dependencies`` directory.

## Commands

Install ``crack.toml`` dependencies, which aren't in the dependencies directory.
It produces ``crack.lock`` and a dependencies directory, if it doesn't exist.

```zsh
crack i
```

Add a rolling dependency to ``crack.toml``, if dependency exists in the repos table.

```zsh
crack a <a repo name from the repos table>
```

Update ``crack.lock`` dependencies.

```zsh
crack u
```

Delete directories, which aren't in ``crack.lock``.

```zsh
crack c
```
