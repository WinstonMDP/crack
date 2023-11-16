# Crack

## Требования

``` toml
name = "package_name"
dependencies.rolling = [
    "https://github.com/monkeyjunglejuice/matrix-emacs-theme.git",
    "https://github.com/agda/agda.git"
]

[[dependencies.commit]]
repository = "https://github.com/jesseleite/nvim-noirbuddy"
commit = "7d92fc64ae4c23213fd06f0464a72de45887b0ba"
```

1. Все пакеты берутся из удалённых репозиториев.
2. Если указан конкретный коммит, то при update пакет не обновляется,
   иначе берётся последний коммит из главной ветки.
3. Конфиг, похожий на cargo.
4. В коде библиотека (модуль) квалифицируется по name.
5. crack.toml, crack.lock.
6. Клонированные репозитории пакетов располагаются в targets/dependencies.
7. update.
8. clean - удаляет пакеты, не указанные в конфиге.

## Задачи

1. Сделать crack.lock.

## Будет прикольно

1. search \<какой-то шаблон\>.
2. new \<package name\> - как в cargo.
