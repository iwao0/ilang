#!/usr/bin/env bash
# BUG_COVERAGE.md を fixture のヘッダコメントから再生成する。
# 各 fixture 先頭の連続する `//` プロセкомメント(expect 等の指示子は除外)を
# 逐語で転記する。要約はしない。
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=crates/ilang-cli/tests/programs
OUT=docs/BUG_COVERAGE.md

emit_desc() {
  # 先頭の連続コメント行を集め、指示子行を除き、1 行に畳んで 200 字で切る
  awk '
    /^[[:space:]]*\/\// {
      line=$0
      sub(/^[[:space:]]*\/\/[[:space:]]?/, "", line)
      if (line ~ /^(expect|expect-error|jit|aot|args|env|skip|timeout|cwd|stdin)[[:space:]]*:/) next
      desc = (desc=="" ? line : desc " " line)
      next
    }
    { exit }
    END { print desc }
  ' "$1" | sed 's/[[:space:]]\+/ /g; s/^ //; s/ $//; s/|/\\|/g' \
    | perl -CSD -ne 'chomp; print length($_) > 180 ? substr($_,0,179) . "\x{2026}" : $_'
}

{
  echo '# 攻撃面カバレッジ索引(全 fixture カタログ)'
  echo
  echo 'バグあぶり出しラウンドが **どの攻撃面を確認済みか** を引くための索引。'
  echo '`crates/ilang-cli/tests/programs/` 配下の全 fixture を、各ファイル先頭の'
  echo 'ヘッダコメント(§10 で「何を pin しているか書く」と定めたもの)を**逐語転記**して並べる。'
  echo
  echo '**役割分担**: 本書は索引(攻撃面 → 確認済みか)、[HANDOFF.md](HANDOFF.md) は履歴と詳細、'
  echo '[BUG_HUNTING.md](BUG_HUNTING.md) は手順。'
  echo
  echo '## 使い方'
  echo
  echo '- **ラウンド開始時**: [BUG_HUNTING.md](BUG_HUNTING.md) §6 で狙う交点を決めたら、本書を grep して'
  echo '  既に近い fixture があるか見る。あれば確認済みなので別の交点へ移る(同じ場所を再 probe しない)。'
  echo '- **fixture を追加したら**: `scripts/gen_bug_coverage.sh` を実行して本書を再生成する。'
  echo '  fixture のヘッダが唯一の真実なので索引は自動で実態に追従する(手動編集しない)。'
  echo
  echo '> このファイルは自動生成物。直接編集せず `scripts/gen_bug_coverage.sh` で再生成すること。'
  echo

  total=0
  for dir in $(find "$ROOT" -mindepth 1 -maxdepth 1 -type d | sort); do
    name=$(basename "$dir")
    files=$(find "$dir" -name '*.il' | sort)
    [ -z "$files" ] && continue
    cnt=$(echo "$files" | wc -l | tr -d ' ')
    total=$((total+cnt))
    echo "## $name ($cnt)"
    echo
    echo '| fixture | pin している内容 |'
    echo '|---|---|'
    while IFS= read -r f; do
      base=$(basename "$f")
      rel=${f}
      desc=$(emit_desc "$f")
      [ -z "$desc" ] && desc='(ヘッダ無し)'
      echo "| [$base]($rel) | $desc |"
    done <<< "$files"
    echo
  done

  # ルート直下の単発 fixture
  for f in $(find "$ROOT" -maxdepth 1 -name '*.il' | sort); do
    base=$(basename "$f")
    desc=$(emit_desc "$f"); [ -z "$desc" ] && desc='(ヘッダ無し)'
    echo "## (root)"
    echo
    echo '| fixture | pin している内容 |'
    echo '|---|---|'
    echo "| [$base]($f) | $desc |"
    echo
    total=$((total+1))
  done

  echo "---"
  echo
  echo "**合計 $total fixture**(自動生成)。"
} > "$OUT"
echo "生成完了: $OUT"
