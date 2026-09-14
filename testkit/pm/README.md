# `gqy pm` 端到端(09-10 分层架构阶段 7)

```sh
BIN=~/.cache/gqy-arch-fixes/target/release/gqy python3 testkit/pm/pm_e2e.py
```

- 离线:两个样例包 `sample-ext`(脚本 + 技能)与 `sample-persona`(人格)都用本地路径装,不碰网络。
  搜索 / 按包名装 / GitHub 来源 / tap 索引要联网,没有夹具,手工验:
  `gqy pm install owner/repo@ref`、`gqy pm search x`、`gqy pm tap add owner/repo`。
- 隔离 `GQY_HOME` 与 `XDG_RUNTIME_DIR`;`tool-call --list` 没 daemon 时本地装配注册表,能直接看到装进来的脚本。
- `gqypm` shim = 指向 gqy 的符号链接,按 argv[0] 识别;打包时做一个 `/usr/bin/gqypm -> gqy` 即可。
- 产物在 `~/.cache/gqy-pm/`。
