# Crack

## Требования

``` toml
name = "package_name"

[[dependencies.rolling]]
repo = "https://github.com/monkeyjunglejuice/matrix-emacs-theme.git",

[[dependencies.rolling]]
repo = "https://github.com/agda/agda.git"
branch = "main"

[[dependencies.commit]]
repo = "https://github.com/jesseleite/nvim-noirbuddy"
commit = "7d92fc64ae4c23213fd06f0464a72de45887b0ba"
```

1. Все пакеты берутся из удалённых репозиториев.
2. Если указан конкретный коммит, то при update пакет не обновляется,
   иначе берётся последний коммит из указанной ветки. Если ветка не
   указана берётся клонируемая ветка (главная).
3. В коде библиотека (модуль) квалифицируется по name.
4. crack.toml, crack.lock.
5. Клонированные репозитории пакетов располагаются в dependencies.
6. update.
7. clean - удаляет пакеты, не указанные в crack.lock.

## Будет прикольно

1. search \<какой-то шаблон\>.
2. new \<package name\> - как в cargo.
