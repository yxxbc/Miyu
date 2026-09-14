#!/usr/bin/env bash
# persona.toml 端到端:给默认人格写一份只留 files 的清单,工具面里脚本与记账应消失;
# 删掉清单后恢复。BIN=<gqy> bash testkit/scripts-migration/manifest_probe.sh
set -u
BIN=${BIN:-target/release/gqy}
REPO=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${OUT:-$HOME/.cache/gqy-scripts-migration}
mkdir -p "$OUT/home"
export GQY_HOME=$OUT/home
# 隔离 IPC socket:否则工具桥会连上本机真 daemon,列的是它的会话工具面
mkdir -p "$OUT/runtime"; export XDG_RUNTIME_DIR=$OUT/runtime
export GQY_SYSTEM_SCRIPTS_DIR=$REPO/src/scripts
manifest=$GQY_HOME/data/personas/default/persona.toml
count() { "$BIN" tool-call --list 2>/dev/null | grep -cE "^(divine|codec|ledger|manage_ledger|remember_fact|use_meme)\b"; }
rm -f "$manifest"
before=$(count)
mkdir -p "$(dirname "$manifest")"
printf '[subsystems]\nmemory = false\n\n[plugins]\nenabled = ["files"]\n' > "$manifest"
after=$(count)
rm -f "$manifest"
restored=$(count)
echo "with-all=$before with-files-only=$after restored=$restored"
[ "$before" -ge 5 ] && [ "$after" -eq 0 ] && [ "$restored" -eq "$before" ] && echo PASS || echo FAIL
