# バグあぶり出しラウンドの手順書

「さらに fixture を追加してバグをあぶり出す」ラウンド(第 8〜23 弾、2026-06)で確立した手法の手順書。**文脈の薄いエージェントが単独で 1 ラウンドを完遂できること**を目的に書いてある。各節の手順は実績に基づき、コマンド・ヘルパー名はすべて実機確認済み。

実装レベルの個別の落とし穴は [HANDOFF.md](HANDOFF.md) の各ラウンド記録と「既知の細かい落とし穴」節を参照。**確認済みの攻撃面は [BUG_COVERAGE.md](BUG_COVERAGE.md) から組合せ側で引ける**(本書は手順、HANDOFF は履歴と教訓、BUG_COVERAGE は攻撃面索引、と役割を分ける)。

## 1. 目的と 1 ラウンドの定義

1 ラウンド = 以下を全部やって main にコミットするまで。途中で止めない(止まってよいのは §2 の停止条件のみ)。

1. 攻撃面を選ぶ(§6)
2. probe を書いて実行する(§3〜§5、構文の罠は §7)
3. バグが出たら修正する(§8)。出なければ「クリーンラウンド」として確認内容を記録する
4. fixture 化する(§10)
5. 検証儀式を回す(§9)
6. HANDOFF に記録し、コミットする(§11)

**クリーンラウンドも成果である。** 第 14・17・23 弾は新規バグゼロだったが、probe した形を pin する fixture を追加し、「この領域は確認済み」と HANDOFF に記録した。これにより次のラウンドが未踏領域へ進める。

## 2. 行動原則(判断規則)

probe の書き方より先にこれを読むこと。ラウンドの品質はほぼここで決まる。

- **期待値を実行前に手計算する。** probe を書いたら、deinit が何回・churn 合計が何になるかをコード上で数えてから走らせる。実測とのズレは「バグ」か「自分の計算ミス」のどちらかであり、必ずどちらかを特定するまで進まない。ズレを「だいたい合ってる」で流すと過剰解放や 1 個 leak を見逃す。
- **「出力なし + exit 0」は正常な場合がある。** `test.expect` だけの fixture は成功時に何も印字しない。成功判定は exit code で行い、出力の有無を根拠にしない。逆に「値は合っているが終了時に SIGABRT(exit 134)」というバグもあった(第 19 弾)— **exit code を毎回確認する**。
- **leak 検出(delta)だけでは過剰解放を見落とす**(第 13 弾の教訓)。過剰解放は「解放されすぎて leak が消える」ため delta=0 に見える。deinit カウント(§3)を過剰解放検出器として必ず併用し、厳密値で突き合わせる。
- **バグを 1 つ直したら同族を全配置で探す。** T→T? 包み coerce の解放漏れは、最初の発見(let)のあと、引数・代入・return 2 経路・field 代入の計 6 箇所に同じ穴があった(第 20〜22 弾)。修正対象と同じ形(同じ coerce / 同じ格納 / 同じ解放)が他のコードパスにないか grep で列挙し、**全部 probe してから**ラウンドを閉じる。
- **新規バグか既存バグかを切り分けてから記録する。** `git stash && cargo build` で HEAD のバイナリを作り、同じ probe を流す。挙動が同じなら既存バグ(自分の変更が原因ではない)。切り分けたら `git stash pop` を忘れない。
- **停止条件: 意味論の選択が必要になったら実装せず止まる。** コピーか共有か、順序を保証するか、エラーにするか動かすか — こうした言語仕様の決定はユーザーのもの。**ilang コードで挙動の違いを示して**選択肢を提示し、回答を待つ。抽象的な説明ではなくコードで示すこと(「Map の反復順が実行ごとに変わる」第 23 弾、「join の意味」第 20 弾はこの形で確認した)。
- **観測した負の結果を隠さない。** 直らないケース、フレーク、説明できない数値は、そのまま報告・記録する。「直りました」と言ってよいのは全数値が説明できたときだけ。

## 3. 計測ツールボックス

### ilang 側ヘルパー(`use std.test as test`)

| 呼び出し | 用途 |
|---|---|
| `test.liveAllocBytes()` | ヒープ生存バイト数。前後差(delta)で leak 検出。内部で event loop を pump 済み |
| `test.liveAllocCount()` | 生存オブジェクト数。AOT 未配線の fixture では `// aot: skip` が要る場合あり |
| `test.liveStringCount()` | 生存文字列数 |
| `test.expect(actual, expected)` | i64 一致検査(不一致で fail) |
| `test.expectTrue(b)` / `expectStr` / `expectF64` | 同上の変種 |

### deinit カウンタ(過剰解放と解放漏れの両方を数える基本パターン)

```rust
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}
```

「この probe で Box は合計 N 個作られ、全部死ぬはずだから deinit は N」と**先に**数える。N より少ない = leak、多い = 過剰解放(use-after-free の前兆)。

### 環境変数(原因調査用)

| 変数 | 出力 |
|---|---|
| `ILANG_HEAP_TRACE=1` | alloc/free を 1 行ずつ(サイズ + ポインタ)。誰の free が欠けた/重複したかの特定に使う |
| `ILANG_MIR_DUMP=1` | lowering 後の MIR。emit された retain / release を目で確認する(第 19 弾はこれで `release v23` の過剰 emit を特定) |
| `ILANG_DEBUG_PROMISE=1` / `ILANG_DEBUG_CLOSURE=1` / `ILANG_DEBUG_TIMER=1` | 各機構の状態遷移ログ |
| `ILANG_HEAP_GUARD=1` | ヒープ破壊の検出を厳しく |
| `ILANG_DUMP_CLIF=1` | Cranelift IR(codegen 層を疑うとき) |

## 4. 計測の罠カタログ

probe の数値が「おかしい」とき、バグと断定する前にここを確認する。全部実際に踏んだ罠。

1. **計測開始後に確保した補助配列が delta に乗る。** `let b = test.liveAllocBytes()` の**あと**に `let acc: i64[] = [0]` を書くと、acc 自身(56 bytes)が leak に見える。補助変数は必ず計測開始**前**に確保する。
2. **文字列リテラルの初回 intern が +1 に見える。** 計測窓の中で初めて使ったリテラルは 1 回だけ intern される。`strings=1` が出たら、反復数を 100→200 に倍にして値が変わらないこと(=一回きり)を確認する。線形に増えるなら本物の leak。
3. **delta が定数なら一回きり、線形なら leak。** 反復数を変えて判別するのが最速。定数 56 や 24 は大抵、初回 intern か計測順の問題。
4. **`liveStringCount` を concat 式の途中で読まない。** `"x" + count.toString()` の最中に読むと中間文字列で +1 がでる。値を一旦ローカルに取り出してから組み立てる。
5. **timer 依存の検証は期限を確実に跨ぐ。** 「5ms 後」を 5ms ちょうどで検査すると負荷次第でフレークする。期限 + 余裕で sleep してから `time.tick()`。
6. **map/set の反復順に依存した合計を書く場合**、挿入順保証(第 23 弾以降)を前提にしてよいが、それ以前のコードを probe するなら順序非依存に書く。

## 5. probe テンプレート

probe は `/tmp/*.il` に書き、`./target/debug/ilang run /tmp/probe.il; echo "exit=$?"` で実行する(ビルドは `cargo build`)。1 probe = 値の正しさ・deinit 数・delta の 3 点セット。

### 単発(3 点セット)

```rust
use std.test as test
let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn target(): i64 {
    let a = new Box(1)          // ← 試したい形をここに
    a.n
}
let acc: i64[] = [0]            // 補助は計測前に確保 (§4-1)
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let v = target()
console.log("val=" + v.toString()
    + " deinits=" + (deinits[0] - bd).toString()
    + " delta=" + (test.liveAllocBytes() - d0).toString())
```

### churn(線形 leak の検出)

```rust
let d1 = test.liveAllocBytes()
let bd1 = deinits[0]
let i = 0
while i < 100 { acc[0] = acc[0] + target(); i = i + 1 }
console.log("churn delta=" + (test.liveAllocBytes() - d1).toString()
    + " deinits=" + (deinits[0] - bd1).toString())
// 期待: delta=0、deinits = (1 回あたりの個数) × 100 ぴったり
```

### 独立性(コピー意味論の検証)

```rust
let a: Box[2] = [new Box(1), new Box(2)]
let o = some(a)        // セルに入れたあと
a[0] = new Box(9)      // 元を書き換える
// o の中身が 1 のままならコピー、9 ならば共有。仕様と一致するか確認
```

共有意味論(let 別名・引数・field 読み)はこの逆 — 書き込みが見えることを確認する。どちらであるべきかが不明瞭なら §2 の停止条件(ユーザーに確認)。

## 6. 攻撃面マトリクス

probe する組合せは次の 3 軸から選ぶ:

- **値の種類**: object / string / 動的配列 / 固定長配列 / Optional / tuple / Map / Set / enum payload / Promise / weak / closure
- **配置**(値がどこへ行くか): let 束縛 / class field / fn 引数 / 戻り値 / セル格納(Optional・tuple・Map 値・配列要素・enum payload・capture)/ 再代入 / 文として破棄 / コンテナ間コピー(slice・union 等)
- **制御フロー**: 早期 return / break / continue / match arm(diverge 含む)/ if join(混合 freshness)/ loop / for-in(早期脱出込み)/ async・await / `?` 演算子

**優先順位は「直近のコミットが触った継ぎ目 × まだ probe していない組合せ」。** 例: 第 20 弾は join 正規化を入れた直後に「join の値 × 全消費位置(return・break・破棄・入れ子・field・tuple)」を突いて 2 系統を発見した。第 22 弾は wrap coerce 修正の直後に「wrap × 全セル格納」を突いて 4 系統を発見した。新機能や修正は必ずその周辺に同族の穴を持っている。

**狙う交点を決めたら、必ず [BUG_COVERAGE.md](BUG_COVERAGE.md) の該当領域を見てから probe を書く。** 既に行があれば確認済みなので別の交点へ移る — 同じ場所の再 probe を防ぐための索引である。表に無い交点が次の候補。HANDOFF を全部読まずに「まだ probe していない組合せ」を判定できるのが本表の目的。

特に効率が良かった形:

- **混合 freshness**: 片腕が `new`、片腕が借用の if / match join(第 20 弾)
- **借用ソースをセルに入れる**: `takeOpt(h.b)`、`let o: Box? = h.b`(第 22 弾)
- **早期脱出が生きている heap 束縛を跨ぐ**: `?` の none 経路・return が `let s = "..." + k.toString()` を跨ぐ(第 11・21 弾)
- **値の持ち出し**: payload を return / break で外へ出す、closure が capture を持って脱出する(第 14 弾)
- **プロセス間の決定性**: 同じ probe を 5 回流して出力が一致するか(第 23 弾で Map 順序の非決定を発見)

## 7. probe を書くときの ilang 構文の罠

probe が parse error で止まる頻出パターン。先に読めば往復が減る。

- **match arm は改行区切り**。`match o { some(v) { v } none { -1 } }` を 1 行に書くと parse error。arm ごとに改行する。
- **ブロック末尾の `-1`** は ASI で直前の式との二項マイナスに解釈される。`return -1` と書く。
- **Result のコンストラクタは `Result.ok(..)` / `Result.err(..)`**。裸の `ok(..)` は undefined function。
- **property は `get name(): T { .. }` / `set name(v: T) { .. }`** をクラス本体に書く形。
- **async**: `await` は async fn 本体のみ。`let x = await f()` で型推論が効かない場合は「Add an explicit `let x: T = ...`」という診断が出るので注釈する。async fn 内の `?` は未対応(Result は分かりにくい型エラー、Optional は明示診断)。
- **`@derive(Eq, Hash)`** がないと class は Set 要素 / Map キーになれない(nominal な診断が出る)。
- **interface は nominal**。`class X: Iface` と明示しないと実装扱いされない。
- **enum payload** は `variant: (T)`(タプル)/ `variant: { f: T }`(構造体)。
- **map 反復**は `m.entries()` / `m.values()`、set は `s.values()`・`s.size()`。

## 8. バグ発見時の手順

1. **最小再現に削る。** probe から無関係な部分を消し、1 つの形だけにする。
2. **HEAD と比較する。** `git stash && cargo build -q` → 同じ probe → `git stash pop && cargo build -q`。既存バグなら記録にそう書く(自分の変更を疑う時間を節約できる)。
3. **MIR を読む。** `ILANG_MIR_DUMP=1 ./target/debug/ilang run probe.il > /tmp/mir.txt 2>&1` で `$main` / 対象 fn の retain / release の emit を確認。「retain が無い」「release が 2 回」をここで掴む。
4. **heap trace で突き合わせる。** `ILANG_HEAP_TRACE=1` で alloc と free を 1 対 1 対応させ、欠けた free / 重複 free / 解放後アクセスを特定する。
5. **修正する。** 修正は対症ではなく原因に対して。解放経路は 1 本ではない — scope sweep / break sweep / `__main` エピローグ / REPL slot 一掃の **4 箇所が同じ規則を要求する**(第 19 弾で 3 箇所直して 4 箇所目を踏んだ)。
6. **同族探索。** 直した形(coerce・格納・解放・retain 判定)と同じパターンを grep で列挙し、**全箇所を probe する**。1 箇所の修正で閉じたラウンドは大抵やり残しがある。
7. **修正前に落ち、修正後に通る fixture を書く**(§10)。

## 9. 検証儀式

ラウンドを閉じる前に全部回す。**どれかを省略する場合は理由を記録に書く**(例: 「docs のみの変更のため AOT / 儀式は省略」)。

```bash
# 1. 全テスト (マイルストーンごとに 1 回。同じ変更へ再実行しない)
cargo nextest run --workspace

# 2. AOT 経路 (lowering / codegen / runtime / checker を触ったら必須。約 2 分)
#    fixture の期待値を更新した場合は「更新後」に回すこと
ILANG_TEST_AOT=1 cargo nextest run -p ilang --test programs run_all_program_fixtures

# 3. nested_generic 並列儀式 (lowering を触ったら。出力なし + exit 0 が正常)
fails=0
for round in 1 2 3 4; do
  for i in $(seq 1 25); do
    ( ./target/debug/ilang run crates/ilang-cli/tests/programs/05_edge_cases/nested_generic.il \
        > /tmp/n_$i.txt 2>&1; echo $? > /tmp/nrc_$i.txt ) &
  done
  wait
  for i in $(seq 1 25); do
    [ "$(cat /tmp/nrc_$i.txt)" != "0" ] || [ -s /tmp/n_$i.txt ] && fails=$((fails+1))
  done
done
echo "nested_generic 100 runs: fails=$fails"   # 期待: fails=0
```

- fixture が落ちたら `target/fixture-failures.log` に詳細(expected / actual / stderr)が残る。
- そのラウンドで書いた probe と、直近ラウンドの fixture を再実行して回帰がないことも確認する。
- 大きな変更(表現の切替など)では「既存 fixture の出力が**変更前と同一**であること」自体が意味論保存の証拠になる。

## 10. fixture 化規約

- 置き場所: `crates/ilang-cli/tests/programs/` 配下。ARC / 端ケースは `05_edge_cases/`、コレクションは `03_collections/` など既存の分類に従う。
- 指示子(ファイル先頭の `//` コメント):
  - `// expect: <stdout の 1 行>` — 複数可、出力と順序一致
  - `// expect-error: <stderr の部分文字列>` — コンパイル失敗を期待する fixture
  - `// jit: skip` / `// aot: skip` — 片経路のみで走らせる(`test.liveAllocCount` 等 AOT 未配線のヘルパーを使う場合は `// aot: skip`)
- **deinit は厳密値で書く**(`>=` 的な緩い検査にしない)。緩めると過剰解放を検出できない。
- churn を 1 本含める(`churn delta=0 deinits=N` の形)。
- ヘッダコメントに「何のバグを pin しているか」「修正前の症状(何が leak / UAF したか)」を書く。初見の読者が fixture の存在理由を理解できること。
- 期待値はでっち上げない。実行して得た値を、手計算と一致することを確認してから書く。

## 11. 記録とコミット

- **HANDOFF.md** に 2 箇所書く:
  1. 「現在地」の直近変更リストに 1 行(「第 N 弾。〜を検出・修正。詳細は下の解決済み記録」)
  2. 解決済み記録セクション(`### [解決済み記録] 第 N 弾: ...` / クリーンなら `[確認済み記録]`)。様式: 検出した症状 → 原因 → 修正(ファイルへの相対リンク付き)→ fixture 名 → 検証結果(nextest / AOT / 儀式)。設計判断待ちが発生したら `[判断待ち記録]` で選択肢ごと書く。
- **[BUG_COVERAGE.md](BUG_COVERAGE.md) の該当領域に 1 行追記する。** 確認した交点・弾・結果(修正/健全/仕様)・代表 fixture を「追記規約」の列で残す。これを怠ると索引が実態とズレて再 probe を招くため、HANDOFF への記録と同格の必須作業とする。
- 挙動が変わったら **docs/syntax.md と docs/syntax_ja.md を同じコミットで更新**する。
- コミット: PR は作らず **main へ直接コミット**。メッセージは英語で、1 行目に変更の本質、本文に症状・原因・修正・検証を書き、末尾を `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` で締める。

## 12. 1 ラウンドのチェックリスト

```
[ ] 直近コミットの継ぎ目から攻撃面を選んだ (§6)
[ ] 狙う交点を BUG_COVERAGE.md で確認した (既出なら別の交点へ) (§6)
[ ] probe の期待 deinit 数・churn 合計を実行前に手計算した (§2)
[ ] 単発 3 点セット + churn + (独立性/共有) を probe した (§5)
[ ] exit code を毎回確認した (silent pass / 終了時 abort の双方を見る)
[ ] 数値のズレを全部説明した (バグ or 計算ミス or §4 の罠)
[ ] バグは HEAD と比較して新規/既存を切り分けた (§8-2)
[ ] 修正したら同族を grep で列挙して全部 probe した (§8-6)
[ ] 意味論の選択が出たら実装せず ilang コードで選択肢を提示した (§2)
[ ] fixture を追加した (厳密 deinit + churn + 由来コメント) (§10)
[ ] cargo nextest run --workspace
[ ] ILANG_TEST_AOT=1 ... run_all_program_fixtures (期待値更新「後」に)
[ ] nested_generic 並列儀式 (lowering を触った場合)
[ ] HANDOFF に記録した (現在地 1 行 + ラウンド記録)
[ ] BUG_COVERAGE.md に確認した交点を 1 行追記した (§11)
[ ] syntax docs を更新した (挙動が変わった場合)
[ ] main へコミットした (Co-Authored-By を付けて)
```
