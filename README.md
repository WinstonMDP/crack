# Crack

Specify dep in ``crack.toml``. ``crack.toml`` must be in the project root.

``crack.toml`` example:

```toml
name = "package_name"

interpreter = "interpreter_absolute_path"
# a default interpreter path is /bin/sanskrit

[[deps]]
repo = "git_repo_url"
# a default branch is git default

[[deps]]
repo = "git_repo_url"
branch = "git_repo_branch"
options = ["feature1", "feature2"]  

[[deps]]
repo = "git_repo_url"
commit = "sha"
option_name = "feature"

[[deps]]
repo = "git_repo_url"
version = "0.1.3"
option_name = "feature"

[[dev_deps]]
repo = "git_repo_url"
```

To include "feature" option from above in installation:

```shell
crack install feature  
```

All deps are stored in ``project_root/deps`` dir.
