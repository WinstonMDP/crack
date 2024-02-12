# Crack

Specify dep in ``crack.toml``. ``crack.toml`` must be in the project root.

``crack.toml`` example:

```toml
name = "package_name"

[[deps]]
repo = "git_repo_url"
# a default branch is git default

[[deps]]
repo = "git_repo_url"
branch = "git_repo_branch"


[[deps]]
repo = "git_repo_url"
commit = "sha"
```

All deps are stored in ``project_root/deps`` dir.

## Commands

Install ``crack.toml`` deps, which aren't in the deps dir.
It produces ``crack.lock``, a deps dir, if it doesn't exist, and
``crack.build``.

```zsh
crack i
```

Update ``crack.lock`` deps.

```zsh
crack u
```

Delete dir, which aren't in ``crack.lock``.

```zsh
crack c
```
