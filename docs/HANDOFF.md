# ilang HANDOFF

新しいセッションへの引き継ぎ用。`/clear` 後にこのファイルを読めば現状の文脈が把握できる構成。

言語仕様の詳細は [`docs/syntax.md`](syntax.md) を参照。このファイルは **「実装の現状」と「次に何をやるか」の引き継ぎ** に絞る。バグあぶり出しラウンドの実施手順(probe の書き方・計測の罠・検証儀式・fixture 規約)は [`docs/BUG_HUNTING.md`](BUG_HUNTING.md) を参照。

## プロジェクト概要

**ilang** はユーザーが新しく設計中のプログラミング言語。最終ゴール:

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する (核となる設計目標)
- **ARC** によるメモリ安全性 (所有権/`mut`/借用は採用しない)
- **JS / TypeScript / Rust 風** のハイブリッド構文。文末は **JS 風 ASI** (改行が `;` 代わり)
- 例外なし。失敗は `Result<T, E>`、回復不能エラーは panic

実装言語: **Rust 1.95**。実行モデル: AST → MIR (SSA) → **Cranelift JIT** が唯一の実行経路。ツリーウォーク インタプリタ (`ilang-eval`) と旧 ilang-codegen 経路は M1 Step 6 で撤去済み。AOT 経由のネイティブ実行は `ilang build` で行う。

## 現在地

`run_all_program_fixtures` (1298/1298) + `cocoa_foundation` + `cocoa_appkit` + workspace 全 539 test 全緑。 `crepr_struct_field_discard.il` (a6e9310e で意図的に赤いまま追加されていた fixture) は緑。 `examples/sdl_breakout/main.il` の起動も実機確認済み (`playing — ESC to quit`)。

直近のセッション (2026-06-14 / 2026-06-11) で main に landing した変更:

- **第 162 弾** (chained getter `a.b.getter` の二重リーク)。 最古領域 `08_properties` を probe して **property getter をレシーバが field/property アクセスの形(`o.inner.boxed`)で読むとリーク**を検出・修正。 2 つの穴: (1) `field_is_property_access`([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs))がレシーバを `this`/Var/クラス名しか解決できず、 **chained レシーバを borrow 扱い**にして leaf getter の +1 が drop されない(コメントに「chained reads は leak」と既知制限として明記されていた)。 (2) getter dispatch([literals.rs](../crates/ilang-mir/src/lower/literals.rs))が **fresh レシーバを解放しない**(`.length` 経路にはある解放が欠落)ため中間 getter 結果(`o.inner`)が漏れる。 修正: 再帰ヘルパ `resolve_static_class_id` で chained Field のクラスを解決し leaf を owned 判定、 getter call 後に `obj_is_fresh && is_arc_heap` で fresh レシーバを Release。 plain field 読みの fresh レシーバ解放(`mk().n` 等)は元から健全と確認。 fixture `chained_getter_receiver_arc.il`。 全儀式緑。 詳細は下の解決済み記録。
- **第 161 弾** (第 159 弾の取りこぼし = 代入 RHS・cast の発散式)。 第 159 弾が消費位置で発散式を拒否したが、 **代入 RHS と cast オペランドを取りこぼし**ていた。 `x = return 5`・`c.n = return 5`・`a[0] = return 5` は checker を素通りし **MIR codegen が Rust panic**(locals.rs/objects.rs/array.rs、 値 join の verifier エラーより悪い)、 `(return 5) as i64` は ugly な lower エラー。 修正: `reject_control_transfer_value` を `ExprKind::Assign` の value・`check_assign_field`/`check_assign_index` の RHS(+ index 代入の obj/index)・`check_cast` のオペランドにも配置。 field/method レシーバ・if/while 条件・await は型不一致で既にクリーン拒否のため対象外。 fixture `control_transfer_in_assign_rhs.il`。 checker のみ・lowering 不変。 詳細は下の解決済み記録。
- **第 160 弾** (クリーンラウンド / 最古領域 overloading の ARC 補強)。 BUG_COVERAGE の追加日を集計し、 最も長く fixture 化されていない **06_overloading / 07_method_overloading**(2026-06-07 以降未更新)を **オーバーロード選択 × heap 引数/戻り値の ARC** で probe — **新規バグなし**。 free-fn overload(param 型・subclass・`T` vs `T?`・heap 返却)も init/method overload(`init(Box)`/`init(Box,Box)`・`add(Box)`/`add(Box,Box)`)も、 選択が正しく heap 引数/戻り値を過不足なく 1 回ずつ deinit(churn で厳密・`ILANG_HEAP_GUARD=1` クリーン)。 既存 overloading fixture は選択の正しさのみで ARC 未検証だったため pin。 また第 107 弾で「壊れている既知の制限」とした **implicit-this generic メソッド**と **generic 返却メソッド**が現在は正常動作する(`generic_method_this_call.il`/`generic_method_returns_generic.il` で pin 済み)ことを確認 — 第 107 弾記録は古い。 fixture 追加のみのため重い儀式は省略(コード変更なし、 JIT 全 fixture + 新 fixture を JIT/AOT で確認)。 fixture: `06_overloading/overload_selection_heap_arc.il`・`07_method_overloading/init_method_overload_heap_arc.il`。
- **第 159 弾** (ユーザー決定 = 発散式を消費位置で checker 拒否)。 155〜158 の発散系統の探索中に、 `f(return 5)`・`1 + (return 2)`・`[1, return 2, 3]` のように **発散式(return/break/continue)を値消費位置**(引数・オペランド・要素等)に置くと、 checker を素通りし `mir lower: no coercion from () to i64` という内部的なメッセージで落ちることを発見。 根は「ilang に never 型が無く `return X` の checker 型が関数戻り値型」(`todo() + 1` と同根)。 ユーザー判断で **never 型は入れず checker で素直に拒否**を選択。 実装: `reject_control_transfer_value`([checker/expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs))を新設し、 二項/単項オペランド・call 引数(`check_call_expr`/`check_args`)・配列/タプル/Map 要素・index・some・template の各消費位置で呼ぶ。 `break`/`continue` はループ外、 `return` はトップレベルでは専用エラーに譲るため `loop_depth`/`ret_ty` を見て well-formed な時だけ拒否。 fixture `control_transfer_not_a_value.il`。 checker のみ・lowering 不変。 詳細は下の解決済み記録。
- **第 158 弾** (if 式の発散分岐が codegen を壊す既存バグ)。 第 157 弾(int/bool match)の同型を **if 式の値 join** で probe して検出。 `let x = if c { 9 } else { return 1 }` という基本形でも `mismatched argument count for jump` でクラッシュ(片分岐が値・片分岐が発散)。 heap・elif 中段発散・引数位置の if でも同様。 原因: `lower_if`([control.rs](../crates/ilang-mir/src/lower/control.rs))が分岐の発散を見ず、 join 型を両分岐の tail から選び両分岐とも join へジャンプ。 発散分岐の死にブロックが `()` を渡し arity 不一致。 **既存バグ**(match と同じく lower_if は発散分岐を一度も扱っていなかった)。 修正: `block_diverges`/`arm_body_diverges` で各分岐の発散を判定し、 join 型は live 分岐のみから選び、 発散分岐は `Unreachable` で閉じて join へジャンプさせない。 fixture `if_diverging_branch_value.il`。 全儀式緑。 詳細は下の解決済み記録。
- **第 157 弾** (int/bool match の発散 arm が codegen を壊す既存バグ)。 第 156 弾の隣接面(発散 × match)を probe して **整数・bool の `match` に発散 arm(`return`/`todo` 等)があると codegen がクラッシュ**を検出。 `match s { 0 { 9 } _ { return 1 } }` という基本形でも `mismatched argument count for jump`。 原因: `lower_match_int`/`lower_match_bool`([match_.rs](../crates/ilang-mir/src/lower/match_.rs))が enum/string 経路と違い `arm_body_diverges` を見ず、 全 arm を無条件で join に push。 発散 arm の死にブロックが `()` プレースホルダを join へ渡し arity 不一致。 **私の変更と独立の既存バグ**(両経路は発散 arm を一度も扱っていなかった、 string 経路のみ正しかった)。 修正: 両経路の各 arm で `arm_body_diverges` を見て発散 arm を join から除外(enum/string と同じ規則)。 fixture `match_int_bool_diverging_arm.il`(int scalar/heap・bool heap・churn deinits=100)。 全儀式緑。 詳細は下の解決済み記録。
- **第 156 弾** (発散値の break が codegen を壊す)。 第 155 弾の隣接。 `break (return 1)` や `break (if c { return 1 } else { return 2 })`(値自体が発散)を実値 break と混ぜると **MIR codegen が Cranelift verifier エラーでクラッシュ**(`mismatched argument count for jump`)。 第 155 は checker を直したが、 MIR の `lower_break` が発散 break 値を Unit プレースホルダとして lower し loop exit param を `()` に固定、 実値 break が i64 を渡して arity 不一致。 修正: (1) `lower_break`([control.rs](../crates/ilang-mir/src/lower/control.rs))で break 値が `arm_body_diverges` なら値を lower 後に死にブロックを `Unreachable` で閉じ、 exit param もジャンプ値も付けない。 (2) MIR の `arm_body_diverges`([match_.rs](../crates/ilang-mir/src/lower/match_.rs))に「else 付き全分岐発散 if」「全 arm 発散 match」を追加。 fixture `loop_break_diverging_value_codegen.il`。 全儀式緑。 詳細は下の解決済み記録。
- **第 155 弾** (`break todo()` が loop 型を Any に固定)。 第 154 弾の同族。 `loop` 内で `break todo()` と実値の `break v` を混ぜると `expected any, got i64`(heap でも `got Box`)でコンパイル不能。 第 154 弾は `todo()` の divergence を match arm と if-join にだけ配線し、 **`loop` の break 値型 join が取り残されていた**。 修正: `Break` 処理([checker/expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs))で break 値が `arm_body_diverges` を満たすなら loop 型に積まない(match/if と同じ規則)。 fixture `loop_break_todo_diverges.il`(scalar/heap 混在 + churn deinits=100)。 全儀式緑。 詳細は下の解決済み記録。
- **第 154 弾** (ユーザー決定 = `todo()` 組み込みを追加)。 LSP の match-arm fill code action が未実装 arm 本体に `todo()` を生成するのに `todo()` が言語に存在せず、 生成コードがコンパイルできなかった。 ユーザー判断で Rust の `todo!()` 相当 — **発散する組み込み・実行時に panic・型検査ではどの期待型にも適合** — を追加。 実装: runtime に `$builtin.todo`(`rt_panic("not yet implemented (todo)")`、 [print.rs](../crates/ilang-runtime/src/print.rs))、 MIR lower で `todo()` を `Inst::Call`(builtin)+ `Terminator::Unreachable` に下げ([call_fn.rs](../crates/ilang-mir/src/lower/call_fn.rs))、 checker は arity 0 を検査し `Type::Any` を返す([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs))、 両 `arm_body_diverges`([checker expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs)・[mir match_.rs](../crates/ilang-mir/src/lower/match_.rs))に `todo()` arm を追加して match join からスキップ、 codegen の PanicAux に builtin を 4 箇所配線(struct・program_decl・jit_symbols・lower_inst/calls)。 `todo` を `is_reserved_global` に追加([sigs.rs](../crates/ilang-types/src/checker/sigs.rs))し再定義を拒否。 match arm 本体・fn body・if/else 分岐・let RHS で発散として通り、 到達すると exit 1 で panic。 fixture `todo_unreached_compiles.il`(到達せず compile + 実行)+ `todo_reached_panics.il`(到達して panic)。 全儀式緑(workspace 548/548・JIT/AOT harness・nested_generic 100/100)。
- **第 149 弾** (zero-await async fn の `return` 型エラー)。 `async fn one(): i64 { return 1 }`(await 無し + `return` 文)が `expected Promise<i64>, got i64` で拒否。 原因は zero-await desugar が tail 式しか `Promise.resolve` で包まず `return X` 文を包まないこと([async_desugar/mod.rs](../crates/ilang-parser/src/normalize/async_desugar/mod.rs))。 修正: 全 `return` を再帰的に wrap(ネスト FnExpr は除外)+ tail は diverge 判定で「1 回だけ包む/包まない」を選択(`Result.ok/err` の arm 跨ぎ推論を保持)。 fixture `async_zero_await_return.il` 追加、 全儀式緑。 詳細は下の解決済み記録。
- **第 153 弾** (REPL の bare 式 echo)。 ユーザー決定で、 REPL の bare 式表示が i64 のみ(`42` は出るが `"hello"`/`true`/`3.14`/配列は無出力)を全型 auto-print に改善。 原因は `run_chunk` が `__main` の i64 戻り値を表示していたこと。 修正: `run` と同じ `wrap_trailing_print`(`console.log(tail)`)を REPL にも適用・i64 echo 撤去([main.rs](../crates/ilang-cli/src/main.rs))。 `console.log(x)` tail は内側 Unit で二重 print せず。 REPL テスト 2 件追加、 14 件回帰なし、 workspace 548/548。 詳細は下の解決済み記録。
- **第 152 弾** (std.events の probe)。 薄い領域 std.events を probe して **emit 中にリスナを `off`/`remove` すると `index out of bounds` で panic** を修正。 `emit` が cached length で生配列を反復していたため、 リスナが emit 中に配列を縮めると範囲外。 両 emit を発火前スナップショット(`slice`)反復に([events.il](../../libs/std/events.il)、 Node.js 準拠)。 fixture `04_modules/events_emit_reentrancy.il`。 詳細は下の解決済み記録。
- **第 151 弾** (std.math の probe)。 薄い領域 std.math を probe して 2 件修正: (1) `sign(±0.0)` が doc の `0.0` でなく `1.0`(`f64::signum` マップ)→ JS `Math.sign` 準拠の専用実装に([math.rs](../crates/ilang-runtime/src/math.rs))。 (2) `min`/`max` の NaN が順序依存・非可換 → doc 通り両側伝播に([math.il](../../libs/std/math.il))。 clamp/lerp/smoothstep/remap・定義域外 intrinsic は健全。 fixture `04_modules/math_sign_and_nan.il`。 `math.il` 変更は再ビルド必須。 詳細は下の解決済み記録。
- **第 150 弾** (break 無し loop tail の divergence)。 `fn f(): i64 { loop { ...return... } }` が「body produces ()」で拒否されていた(checker が break 皆無の loop を divergent でなく `()` 扱い)。 第 149 弾で表面化した async/非 async 共通の制限。 修正: `Loop(None)`(break 皆無)を `ret_ty` として型付け([checker/expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs)、 第 144 弾と同じ近似)。 無限ループ `loop {}` も Rust 同様に通る。 fixture `loop_no_break_diverges.il` 追加、 全儀式緑。 詳細は下の解決済み記録。
- **第 148 弾** (weak 強制を全消費サイトで完了)。 第 145〜147 は let/再代入/return を直したが、 **コンテナ/引数の消費サイト**(field store・array.push・array/tuple リテラル・`T.weak` 引数)が fresh ソースでまだ UAF だった。 coerce 集約を試みたが、 サイトにより「coerce が retain 済みと仮定(リテラル)」「自前 retain(field/array/arg)」が混在し二重 retain でリーク(fixture 2 件回帰)→ `e0edf298` へ戻し、 ユーザー合意で **per-site 順序修正**(retain を release より前に足すのみ・除去なし)へ。 全消費サイトに適用、 fixture `weak_fresh_into_container_and_arg.il` 追加、 網羅スイープ全緑、 全儀式緑。 詳細は下の解決済み記録。
- **第 147 弾** (weak 強制の残り経路を完了)。 第 145/146 は borrowed ソースだけ直しており、 **fresh ソース**(`new T()`・if/else / match join)と **return** がまだ UAF だった。 原因は (a) `is_fresh_object_expr` が If/Match/New を fresh 分類するため第 145 弾の Weak 除外が短絡されたこと、 (b) weak retain を strong 解放より後に出す**順序バグ**。 修正: `StrongToWeak` ではソースの fresh/borrowed を問わず **strong 解放より前に weak +1 を取得**(let/再代入/return の 3 経路)。 fixture 2 件追加(通常モードで決定的)、 網羅スイープ全緑、 全儀式緑。 詳細は下の解決済み記録。
- **第 146 弾** (weak 再代入 UAF + 一時 weak レシーバのリーク)。 第 145 弾が露呈した兄弟問題 2 件。 (1) 再代入 `w = strongRef` も同じ「coercion=fresh」で weak retain を省き UAF([expr.rs](../crates/ilang-mir/src/lower/expr.rs))。 (2) **第 145 弾の回帰**: `mkWeak().get()` の一時 weak レシーバが解放されずリーク([calls/mod.rs](../crates/ilang-mir/src/lower/calls/mod.rs) に fresh-release ガード追加)。 fixture 2 件追加。 詳細は下の解決済み記録。
- **第 145 弾** (weak 束縛の UAF を修正)。 `let w: T.weak = strongRef` が weak 共有を retain せず、 zombie box が weak 生存中に解放され `w.get()` が解放済みメモリを読む UAF(通常は偶然 0 を読み `none`、 HEAP_GUARD で `some` に化ける)。 原因は `let` 束縛の retain 判定が「coercion=新セル +1 の wrap」と決め打ちし、 +1 を作らない bare な `StrongToWeak` まで fresh 扱いして束縛 retain を省いたこと([stmt.rs](../crates/ilang-mir/src/lower/stmt.rs))。 修正は判定から bare `Weak` 標的を除外(`StrongToWeak` のみ該当、 Optional<Weak> は別経路で不変)。 修正案 3 種を比較し局所案を採用。 fixture `05_edge_cases/weak_bind_keeps_zombie_alive.il`(解放スロット再利用で通常モードでも決定的に落ちる)追加、 リーク無し確認、 workspace 546/546 + JIT/AOT + nested_generic 100/100 緑。 詳細は下の解決済み記録。
- **第 144 弾** (match の divergence 解析を if/else と整合)。 全 arm が `return`(/`break`/`continue`)する網羅的 match を関数末尾に置くと `body produces ()` で誤って拒否されていた(if/else 同等形は通る)。 原因は match 結果型が diverge arm を join からスキップし、 全 arm diverge 時に `result_ty=None`→`Type::Unit` に落ちていたこと。 修正: diverge arm の見せかけ型を保持し非 diverge arm が無ければ採用([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)、 enum/Optional 両 path)。 `?` desugar の混在ケースは不変。 fixture `06_enums/match_all_arms_return_tail.il` 追加、 workspace 546/546 + programs JIT/AOT 緑。 詳細は下の解決済み記録。
- **第 143 弾** (capability の抜け穴を塞いだ)。 第 142 弾の enforcement に bypass: `let f = abs` で `@extern(C)` 関数(ffi シンク)を値に束ね `f(-7)` と**間接呼び出し**すると ffi ゲートを回避できた(`make_closure`+`call_indirect` に lower され、 直接 `Inst::Call` だけ見ていたゲートを素通り)。 当時 AOT がたまたまコンパイルエラーになったのは無関係な `libc.doAbort` を拾った巻き添えで、 自己完結 extern では JIT・AOT 両方が素通り。 修正: `call_cap` に `MakeClosure`/`FuncAddr` を追加し、 **extern のアドレス materialize 地点で cap を課す**(非 extern wrapper は exempt)。 過剰ゲート無しを確認(`ffi` 付与で間接呼び出しが実行)。 fixture `11_capabilities/ffi_indirect_{denied,granted}/` 追加、 workspace 546/546 + programs JIT/AOT 緑。 詳細は下の解決済み記録。
- **第 142 弾** (ユーザー決定 = capability enforcement を実装)。 言語の中核設計目標 **capability ベースのセキュリティ**を初めて enforce(従来 `@requires` はパースのみ・未 enforce)。 ユーザー判断で **コードの `@requires` でなく `ilang.toml` マニフェスト方式**(`capabilities = ["file","os","ffi","net"]`)、 **JIT は実行時エラー・AOT はコンパイルエラー・deny by default**(toml 未記載/無しは拒否)を選択。 実装: 真のシンクは `@extern(C)`/`@intrinsic` 呼び出し(MIR で `FuncRef::Local(extern-kind fn)` / `FuncRef::Extern`)で、 必要 cap は **callee の C シンボル**で決定(`$fs.*`→file・`$os.*`→os・他 `$…` 内部 intrinsic→exempt・非`$` 実 C シンボル→ffi)— inline 非依存。 新パス `cap_gate`([passes/cap_gate.rs](../crates/ilang-mir/src/passes/cap_gate.rs)): JIT は各シンク呼び出し前に no-arg `cap_require_*` builtin を挿入、 AOT は `required_caps` を集計。 runtime([caps.rs](../crates/ilang-runtime/src/caps.rs)): granted ビットセット(`set_granted`)と `__cap_require_file/os/ffi/net`(未許可なら `rt_panic` で exit 1)。 builtin を 4 箇所配線(jit_symbols・PanicAux struct・program_decl の `declare_unit_void`・calls.rs)。 CLI([manifest.rs](../crates/ilang-cli/src/manifest.rs)): entry を **canonicalize して上方探索**で `ilang.toml` を発見、 `capabilities` を parse(既存 `[package]`/`[deps]` と同居、 未知 cap はエラー)。 run_file は `set_granted` + `insert_gates`、 build_file は `check_caps` で未許可なら**コンパイルエラー**。 互換: deny-by-default のため既存の全 `ilang.toml`(48 個)に `capabilities` を top-level で追記(リポジトリの全コードは信頼)。 std.fs/os は内部で `@intrinsic`(`@extern(C)` でなく)を使う点・inline がシンクを user 関数へ移す点・既存 `ilang.toml` がプロジェクト manifest だった点・TOML テーブル後置の罠を解決。 検証: file/os/ffi の deny→grant・AOT コンパイルエラー・math は exempt・pure はマニフェスト不要・unknown cap エラーを統合テスト 6 件で確認。 nextest 540/540 + capability 6 件、 AOT 全 fixture PASS、 nested_generic 100/100。 docs に capability 節を追記。 fixture/test: `crates/ilang-cli/tests/capabilities.rs`。 詳細は下の解決済み記録。
- **第 141 弾** (クリーンラウンド / 薄い領域の補強)。 第 140 弾で薄いと特定した **timer**(setInterval=2 fixture)を深掘り probe — **新規バグなし**。 setInterval が **re-arm して複数回発火**(3 回後に自己 `clearInterval`、 count=3・sum=21)・`clearTimeout` が発火前にキャンセル・既 clear id の double-clear が no-op・interval/timeout コールバックの **heap capture ARC**(interval が capture を複数発火に跨いで保持、 timeout を 50 個 churn で全 capture が 1 回ずつ deinit・delta=0・`ILANG_HEAP_GUARD=1` クリーン)が全て健全。 既存 `timer_microtask_order.il` が順序/microtask/0ms 連鎖/re-entrant を、 `live_alloc_probe_nonblocking.il` が setInterval+clearInterval(初回 clear)を網羅していたが、 **複数回発火・キャンセル・capture ARC** は未 pin だったため fixture 化(3 回 JIT + AOT で安定)。 fixture 追加のみのため重い儀式は省略(直前の第 135 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `05_edge_cases/timer_interval_multifire_arc.il`。 **薄い領域の残り**: SIMD(レーン値が ilang から観測不能でテスト面が構造的に狭い)、 capability/@requires(未 enforce の将来機能)、 @com/@handle(Windows 専用)。
- **第 140 弾** (クリーンラウンド / 薄い領域の補強)。 テスト被覆を機能横断で集計し、 **実在機能で fixture が薄い領域**を特定(Regex=1、 SIMD=2、 setInterval=2、 @packed=1。 capability/@requires=0 は未 enforce の将来機能、 @com/@handle は Windows 専用で darwin 検証不可)。 最も「広い API なのに 1 fixture」の **std.regex** を深掘り probe — **新規バグなし**。 キャプチャ群置換(`$3/$2/$1`)・複数群・`$$` リテラル `$`・フラグ m(`^`/`$` 行境界)・s(dot が改行一致)・x(空白無視)・i+m 合成・split の前後マッチ(空文字含む)・zero-width(空パターン)マッチ・unicode `\w`・findAll の heap 文字列配列と Regex オブジェクトの churn ARC(delta=0)が全て健全(Rust regex crate ラップ + marshalling が正しい)。 既存 `regex_basic.il` が test/find/findAll/replace/split + i フラグのみだったので、 キャプチャ群・m/s/x フラグ・ARC・zero-width を fixture 化。 fixture 追加のみのため重い儀式は省略(直前の第 135 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `04_modules/regex_capture_flags_arc.il`。
- **第 139 弾** (クリーンラウンド)。 **unicode 文字列**を広く sweep — **新規バグなし**。 全 string メソッド(length/charAt/slice/indexOf/lastIndexOf/startsWith/endsWith/includes/split/replace/trim/toUpper/toLower/hashCode)が**コードポイント単位**で一貫し、 多バイト(`あ`)・astral plane(`😀` = 4 バイト/1 コードポイント)・結合文字(decomposed `e\u{0301}` = 2 コードポイント、 NFC 正規化なしで precomposed `\u{00E9}` と不一致)・`\u{...}` コードポイントエスケープ・埋め込み NUL(長さ前置で生存)・空区切り split(コードポイント分割)・多バイト区切り split・全置換 replace(コードポイント長変化)・unicode の Set/Map キーが全て健全。 既存 `string_edge.il` は length/charAt、 `string_split_and_escape_edges.il` は `\r\n\t\0` を網羅していたが、 `\u{}` エスケープ・結合文字の no-normalization・lastIndexOf fromIndex・astral slice が未 pin だったため fixture 化。 fixture 追加のみのため重い儀式は省略(直前の第 135 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `05_edge_cases/string_unicode_escapes_and_combining.il`。
- **第 138 弾** (クリーンラウンド)。 まだ触れていない値種別 **weak 参照**を新機能(構造的 ==・enum eq/hash・enum Set キー)と組み合わせて sweep — **新規バグなし**。 weak はこれらの構造的等価で**参照比較**される(同一ターゲットの 2 つの weak は等しい、 別ターゲットは異なる)。 tuple 内の weak スロットの `==`/`!=`、 weak payload を持つ enum の `==`(同/別ターゲット・nil)、 **enum-weak-payload を Set 要素**にした構造的 dedup(同一ターゲットで重複排除・`has`)、 その後の `weak.get()` upgrade が全て健全。 @extern(C) struct の f32 フィールド(f64 リテラルから)・generic クラスの f32 フィールド・int→float coercion も健全と確認(第 134/135 弾の波及先)。 weak × 構造的 eq × payload-enum キーの cross-feature 組合せが未踏だったため pin。 fixture 追加のみのため重い儀式は省略(直前の第 135 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `05_edge_cases/weak_in_structural_equality.il`。
- **第 137 弾** (クリーンラウンド)。 数値 coercion 系統を離れて async/await の heap ARC を sweep — **新規バグなし**。 await ループでの heap 蓄積(`Box[]` に push)・Optional<heap> 返却・rejection 時の await 前 heap 解放・nested async(async が async を await し双方 heap 返却)が全て deinit 厳密・`ILANG_HEAP_GUARD=1` クリーンで健全(第 100/101 弾で hardening 済み、 generic クラスの f32 フィールド・int→float coercion も健全と確認)。 既存 async fixture が網羅していなかった **async fn が `Optional<heap>` を返す**形(suspend する state machine が some/none で heap を wrap、 await 跨ぎの held box と some-path の返却 box を厳密に回収)のみ pin。 fixture 追加のみのため重い儀式は省略(直前の第 135 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `04_modules/async_optional_heap_return.il`。
- **第 136 弾** (クリーンラウンド)。 第 134/135 弾の f64→f32 demote の周辺を残り全ての構築/格納位置で sweep — **新規バグなし**。 f32 の dynamic/固定配列リテラル・配列要素 store・enum tuple payload・enum struct payload・tuple リテラル・`Set<f32>` 要素(構造的 dedup)・Map 値・デフォルト引数(省略/明示)・closure 返却・変数再代入が全て f64 リテラルから正しく demote(第 134 弾の field・第 135 弾の optional wrap 以外は元から `lower_arg_to` 等の coerce 経路で健全)。 将来 coercion 配線を変えても回帰しないよう全位置を pin。 fixture 追加のみのため重い儀式は省略(直前の第 135 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `05_edge_cases/f32_from_f64_all_positions.il`。
- **第 135 弾**。 第 134 弾(f32 フィールド)の同族(scalar 値の他の格納先)を sweep して **f64 値を `f32?`(numeric optional)に代入すると SIGSEGV** するバグを検出・修正。 配列要素・Map 値・再代入・return・tuple の f32 は元から coerce 経路で健全だったが、 **numeric Optional への wrap だけ**が穴。 `class C { a: f32?  init() { this.a = 8.5 } }` で raw f64 が f32? スロットに入り read/release でクラッシュ(explicit `8.5f32` や `f64?` は正常)。 原因: Optional-wrap 判定([coerce.rs](../crates/ilang-mir/src/lower/coerce.rs) と `store_value_to_field`([expr.rs](../crates/ilang-mir/src/lower/expr.rs)))が `inner == 値型` の**完全一致**を要求し、 inner=F32・値=F64 で wrap も demote もされなかった。 修正: 両所の wrap 条件に「inner と値が両方 numeric で異なる」を追加し、 box 前に値を inner 型へ coerce(f64→f32 は fdemote)。 f32? フィールド(init/some 代入/none)・`let f32?`・引数・return・i32→i64? 拡大・f32? の array/Map を JIT/AOT + `ILANG_HEAP_GUARD=1` で確認。 **新規/既存**: numeric optional 導入以来の既存バグ。 既存テストは explicit f32 リテラルや同型 optional のみだった。 fixture: `05_edge_cases/numeric_optional_wrap_coerce.il`。 検証: nextest 540/540、 AOT 全 fixture PASS、 nested_generic 100/100。 詳細は下の解決済み記録。
- **第 134 弾**。 第 132 弾の表示系統の同族(値が文脈ごとに別経路を通る)を float で sweep して **f64 値を `f32` フィールドに直接代入すると 0.0 になる**バグを検出・修正。 `class C { x: f32  init() { this.x = 1.5 } }` で `c.x` が 1.5 でなく **0.0**(値自体が壊れる。 print でなく store/load)。 直接・tuple・optional・array の f32 は正常、 **f32 フィールドだけ**全滅。 原因: object フィールドは 8 バイトスロット。 `store_value_to_field`([expr.rs](../crates/ilang-mir/src/lower/expr.rs))の scalar fall-through が値を**フィールド型へ coerce せず**そのまま store するため、 f64 リテラル `1.5`(= `0x3FF8000000000000`)を 8 バイトで書き、 load は f32 として下位 32bit(= `0x00000000` = 0.0)を読む不一致。 f64 フィールドは 8 バイトで一致するため無事、 i64/i32 も extend で無事だった。 修正: fall-through に「`vty != fty` かつ両方 numeric なら `self.coerce`」を追加(f64→f32 は fdemote)。 **新規/既存**: f32 フィールド導入以来の既存バグ。 既存の f32 フィールド fixture(`class_derive_float_field.il` 等)は全て **explicit `1.0f32`** リテラルや `init(x: f32)` の引数 coerce 経由で値が既に f32 だったため、 「f64 値を直代入」する穴が未踏だった。 f32 フィールドの arithmetic(`len2`=x²+y²)・後設定・f64 式からの代入・object 配列内の f32・fpromote(f32→f64 フィールド)・int フィールド(i32 from i64 const)非回帰を JIT/AOT で確認。 fixture: `02_classes/f32_field_from_f64_value.il`。 検証: nextest 540/540、 AOT 全 fixture PASS、 nested_generic 100/100。 syntax docs は f32 フィールドの内部表現のため変更なし。 詳細は下の解決済み記録。
- **第 133 弾** (クリーンラウンド)。 第 132 弾の u64 符号性修正の周辺(unsigned 整数の端)を sweep — **新規バグなし**。 u64 を境界値リテラルと比較(`big > 9e18`・`big < 18e18+1`・`big >= 18e18`・`==`/`!=` — リテラルが u64 を adopt し境界跨ぎでも unsigned 比較)、 narrow unsigned(u8/u16/u32)のリテラル除算/剰余/シフト(ゼロ拡張で元から正・回帰防止)、 u64↔i64 キャスト(最上位ビット立ちで負 i64・`-1 as u64`=2^64-1・`9e18 as i64` は正で fits)・u64 narrowing(下位 32bit 保持)が全て健全。 第 132 弾 fixture が未カバーの境界比較・narrow unsigned 除算・キャストを pin。 fixture 追加のみのため重い儀式は省略(直前の第 132 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `01_basics/unsigned_literal_and_cast.il`。
- **第 132 弾**。 値等価系統から離れて数値 coercion を probe し、 **u64 の符号性に関する 2 つの既存バグ**を検出・修正。 (1) **`u64_val / 2`(`% 2`・`>> 2`)が符号付き演算になる** — 裸の整数リテラル `2` は u64 と同幅で MIR に i64 として届き、 `unify_numeric`([coerce.rs](../crates/ilang-mir/src/lower/coerce.rs))の cross-sign tie-break が符号付きを優先するため IDivS/IRemS/IShrS を使用、 値 ≥ 2^63 で誤答(`18e18/2` が負)。 (2) **u64 の表示が符号付き** — `toString`/`console.log`/配列/`${}` が i64 ビットを符号付き 10 進で出力、 `18e18` が負表示(Map 値だけは `PK_I64_UNS` 経由で正しかった)。 比較は元から unsigned 正常。 修正(1): `lower_binary`([ops.rs](../crates/ilang-mir/src/lower/ops.rs))で、 一方が整数リテラルで同幅 cross-sign のときリテラルを相手の型に retype(checker のリテラル adopt を MIR で再現、 ビット不変なので値変換不要)。 変数どうしの cross-sign(FFI の size_t↔i64)は不変。 修正(2): runtime に `__uint_to_string`/`__print_uint`/`__fmt_uint`([strings.rs](../crates/ilang-runtime/src/strings.rs)/[print.rs](../crates/ilang-runtime/src/print.rs)/[fmt.rs](../crates/ilang-runtime/src/fmt.rs))を追加し、 toString([scalar.rs](../crates/ilang-mir/src/lower/calls/scalar.rs))・console.log/配列([print_emit.rs](../crates/ilang-mir-codegen/src/compile/print_emit.rs))・`${}`([fmt_emit.rs](../crates/ilang-mir-codegen/src/compile/fmt_emit.rs))の `MirTy::U64` を unsigned フォーマッタへ振り分け(narrow unsigned は i64 へゼロ拡張で正なので従来どおり)。 builtin を各 3 箇所配線。 **新規/既存**: u64 導入以来の既存バグ。 第 93/94 弾は u64 を値 < 2^63 でテストし、 リテラル除算と ≥ 2^63 の表示を見逃していた。 div/mod/shift(リテラル)が unsigned・符号付き i64 の div/mod/shift 非回帰・全表示経路 unsigned・比較 unsigned 非回帰を JIT/AOT で確認。 fixture: `01_basics/u64_signedness.il`。 検証: nextest 540/540、 AOT 全 fixture PASS、 nested_generic 100/100。 docs 更新。 詳細は下の解決済み記録。
- **第 131 弾** (クリーンラウンド)。 第 130 弾の payload enum Set/Map キーの周辺を sweep — **新規バグなし**。 set 演算(union/intersection/difference)・述語(isSubsetOf/isSupersetOf/isDisjointFrom)・Map `entries()` が payload enum 要素で全て正しく動き、 set 演算が結果セットの enum キーを **種別対応 ARC** で retain/release する(heap payload enum の union を churn で delta=0・deinit 厳密・`ILANG_HEAP_GUARD=1` クリーン)。 enum キー Map の print も正常(`{H::tag(a): 1}`)。 第 130 弾 fixture は add/has/delete/forEach/keys のみで、 set 演算・述語・entries が未 pin だった。 fixture 追加のみのため重い儀式は省略(直前の第 130 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `03_collections/set_payload_enum_operations.il`。
- **第 130 弾** (ユーザー決定 = payload enum を直接 Set/Map キーに)。 第 129 弾の兄弟経路を probe して **enum に equals/hashCode を付けたのに payload enum を直接 `Set<Shape>` 要素 / Map キーにできない**不整合を検出。 enum フィールドを @derive キークラスに包めばキーになれるのに、 payload enum 単体は「class Shape must declare equals/hashCode」で拒否(`enum_is_value_keyable` が unit/@flags のみ許可していた)。 ユーザー判断で **payload enum を直接 Set/Map キーに**(第 129 弾の自然な完成)。 実装(中規模): runtime の object-keyed store(`ObjectStore`/`ObjectMapStore`)に `key_kind` を追加し、 全キー ARC 箇所(insert/remove/clear + values/forEach/keys/entries/set 演算の iteration、 計 ~15 箇所)を `__retain_object` 固定から `retain_field_by_kind`/`release_field_by_kind`(種別ディスパッチ)へ変更 — enum キー(`KIND_ENUM`)の rc を正しいオフセットで触る(object 固定だとメモリ破壊)。 専用 constructor `__set_new_enum()`/`__map_new_enum()`([sets.rs](../crates/ilang-runtime/src/sets.rs)/[maps.rs](../crates/ilang-runtime/src/maps.rs))が `__enum_structural_eq`/`__enum_structural_hash` と `KIND_ENUM` を内部配線(fnptr 受け渡し不要)。 enum 用の print kind `PK_ENUM`([kind.rs](../crates/ilang-runtime/src/kind.rs))を新設し print_dispatch が `format_enum_into` で整形。 checker(`enum_is_value_keyable` → 全 enum 許可)、 lowering([expr.rs](../crates/ilang-mir/src/lower/expr.rs))が payload enum(`enum_has_payload`)の Set/Map を新 constructor へ振り分け(unit/@flags は従来の i64-tag store)。 builtin を 4 箇所配線。 Set dedup(構造的・オブジェクト payload は参照)・Map キー上書き・delete・has・forEach/keys iteration・heap payload enum の churn ARC(delta=0・`ILANG_HEAP_GUARD=1` クリーン)・print(`Set { Shape::circle(9), ... }`)を確認。 旧 expect-error fixture `set_of_payload_enum_error.il`(拒否を主張)を削除。 docs 更新。 fixture: `03_collections/set_map_payload_enum_key.il`。 検証: nextest 540/540、 AOT 全 fixture PASS、 nested_generic 100/100。 詳細は下の解決済み記録。
- **第 129 弾** (ユーザー決定 = enum フィールドを @derive で完全支援)。 `@derive(Eq, Hash)` のフィールド型を probe して **enum フィールドを持つクラスに @derive を付けると「undefined class」という誤診断**が出るバグを検出・修正。 array/optional フィールドは正しく「unsupported field type」と言うのに、 enum フィールドだけ別経路で誤報告(合成 equals が `this.f.equals(...)` を生成し enum を class 扱いで解決して失敗)。 ユーザー判断で **enum を @derive(Eq, Hash) で完全支援**(enum フィールドのクラスがそのまま Set/Map キーになれる)を選択。 実装: enum 値に組込みメソッド `.equals(other): bool` / `.hashCode(): i64` を追加し、 derive の合成コードを無変更で通す方式。 runtime に `__enum_structural_hash`([enums.rs](../crates/ilang-runtime/src/enums.rs))と汎用ハッシュ `value_structural_hash`([equality.rs](../crates/ilang-runtime/src/equality.rs))を新設 — 判別子 + payload を `__enum_structural_eq` と同じ kind 分岐(数値=値・string=str hash・enum/tuple/array/optional=構造的再帰・参照型=ポインタ)で畳み込み、 等価な値は等価ハッシュ。 checker([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs))が enum 受信側(`Type::Object` かつ `self.enums` 登録済み)の hashCode→I64・equals→Bool を認識。 lowering([scalar.rs](../crates/ilang-mir/src/lower/calls/scalar.rs))が `__enum_structural_hash`/`__enum_structural_eq` へ振り分け(fresh 受信側は dispatcher で、 fresh equals 引数はこの場で解放)。 builtin `enum_structural_hash` を 4 箇所配線。 unit/数値/文字列/複数 slot payload の enum フィールドで Set dedup・Map キー上書き・hash/eq 一貫・standalone enum equals/hashCode・fresh key の churn ARC(delta=0・HEAP_GUARD クリーン)を確認。 array/optional フィールドは従来通り clean に拒否(診断メッセージに「enums」を追記)。 docs(syntax.md / syntax_ja.md)に enum の equals/hashCode を追記。 fixture: `02_classes/derive_enum_field.il`。 検証: nextest 540/540、 AOT 全 fixture PASS、 nested_generic 100/100。 詳細は下の解決済み記録。
- **第 128 弾** (クリーンラウンド)。 第 127 弾の構造的 `==` の周辺を未カバーの形で sweep — **新規バグなし**。 3 段相互再帰(tuple of optional of array、 array of tuple-of-optional、 optional of array-of-tuple)で `==`/`!=` が各 kind を再帰的に正しく比較、 条件位置(`if`/`while`/`match` guard)での使用、 参照型スロット(tuple/optional 内の Map・object はポインタ比較で `(m,5)==(m,5)` 同一参照=true・別参照=false)、 空/単一要素配列が全て健全。 固定長配列 `==` は別レイアウトのため意図通り型エラー(`i64[2]` 同士で「cannot apply binary op」)を expect-error で pin。 **既知の制限を記録**: tuple 内の `none` は注釈なしだと `any?` と推論され tuple 型が一致しないため、 `(some(x),3) != (none,3)` 形は absent スロットへ型注釈が要る(`==` 自体でなく `none` の型推論の既存制限、 標準的に注釈で解決)。 fixture 追加のみのため重い儀式は省略(直前の第 127 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `05_edge_cases/structural_eq_nested_and_refs.il`・`05_edge_cases/fixed_array_equality_error.il`。
- **第 127 弾** (ユーザー決定 = 構造的 `==` を実装)。 第 126 弾の続きで `==` を使う他経路を probe し、 **`==` が未定義の型(tuple・動的配列・optional)の配列 `includes`/`indexOf`/`remove` がコンパイルを通り常に false/-1 を返す**不整合を検出。 `(1,2)==(1,2)` は checker が拒否するのに `[(1,2)].includes((1,2))` は通って false(fresh tuple は毎回別ポインタ)。 ユーザー判断で **tuple・動的配列・optional に構造的 `==` を導入**(enum/string と同じ「値で一致」を全 heap 値型へ拡張)。 実装: runtime に共通ディスパッチャ `value_structural_eq(a,b,kind)` と `__tuple/array/optional_structural_eq` を新設([equality.rs](../crates/ilang-runtime/src/equality.rs))— tuple は packed ワードの要素 kind、 array はヘッダの elem kind、 optional はセルの inner kind を読み、 数値はビット・文字列/enum/tuple/array/optional は構造的(再帰)・object 等の参照型はポインタで比較。 `cell_matches`([arrays.rs](../crates/ilang-runtime/src/arrays.rs))と enum payload 比較([enums.rs](../crates/ilang-runtime/src/enums.rs))もこのディスパッチャに統一(enum の tuple/array payload も構造的になる副次改善)。 `==`/`!=` 演算子: checker([expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs))が同型 tuple・動的 array・optional(`none` は inner Any 許容)を許可、 lowering([ops.rs](../crates/ilang-mir/src/lower/ops.rs))が per-type builtin へ振り分け(enum と同様に Ne は否定、 fresh オペランド解放)、 builtin を 4 箇所配線(jit_symbols・mod.rs struct・program_decl・calls.rs)。 固定長配列はレイアウトが異なるため対象外(動的配列のみ)。 順序比較(`<`等)は型エラーのまま。 tuple(入れ子・文字列スロット・heap スロットは参照)・array(長さ違い・文字列要素)・optional(some/none・`x==none`)・配列検索(tuple/array/optional 要素)・fresh オペランドの churn ARC(delta=0・HEAP_GUARD クリーン)を確認。 旧テスト `array_equality_rejected`(配列 == 拒否を主張)を新挙動(構造的・Bool)へ更新。 docs(syntax.md / syntax_ja.md)に構造的 == を追記。 fixture: `05_edge_cases/structural_eq_tuple_array_optional.il`・`03_collections/array_search_structural_containers.il`。 検証: nextest 540/540、 AOT 全 fixture PASS、 nested_generic 100/100。 詳細は下の解決済み記録。
- **第 126 弾**。 第 105 弾(enum `==` の構造的比較)の兄弟経路を probe して **配列 `indexOf`/`includes`/`remove` が payload enum をポインタ比較し構造的に一致しない**バグを検出・修正。 `[Shape.circle(5), ...].includes(Shape.circle(5))` が **false**、 `indexOf(Shape.circle(9))` が **-1**(配列に在るのに)。 unit variant(`dot`)だけは interned singleton で同一ポインタのため偶然動き、 payload variant は構築毎に新規 box されポインタが異なり全滅。 直接 `==` は第 105 弾で構造的なのに配列検索だけ取り残されていた。 原因: runtime の `cell_matches`([arrays.rs](../crates/ilang-runtime/src/arrays.rs))が `stored == needle`(ビット/ポインタ)と `KIND_STR`(構造的)しか分岐せず、 **KIND_ENUM のケースが無い**ため enum はポインタ比較に落ちていた。 第 105 弾の `__enum_structural_eq`(判別子 + payload kind を再帰比較)を KIND_ENUM 要素で呼ぶ分岐を追加 — `indexOf`/`includes`/`remove` は同じ `cell_matches` を通るので一括で是正。 単数/複数 slot payload・tuple payload・unit variant・否定・`remove` の最初の構造的一致削除・fresh enum needle の ARC(churn delta=0、 borrow のみで非リーク)を確認。 **新規/既存**: 配列検索は第 105 弾以前からポインタ比較で payload enum を見つけられず(既存の振る舞い)、 第 105 弾が直接 `==` を構造的にしたことで `==` と配列検索の**不整合**が生じていた — 今回 `==` 側に揃えた。 enum は `@derive(Eq, Hash)` 不可で Set/Map キーには非対応(clean error、 別経路の穴は無い)。 fixture: `03_collections/array_search_enum_structural.il`。 検証: nextest 539/539、 AOT 全 fixture PASS(runtime のみ変更のため nested_generic 儀式は省略)。 詳細は下の解決済み記録。
- **第 125 弾** (クリーンラウンド)。 第 124 弾の `is`/`as?` 修正の周辺を **generic クラス instance** で sweep — **新規バグなし**。 `class Box<T>: Sized` の instance に対する instance→interface の `is`/`as?`(`s is Sized`・`s as? Box<i64>`)、 **型引数の判別**(`Box<i64> as? Box<string>`/`is Box<string>` が none/false = monomorphize 後の別 class-id を正しく区別)、 混在 interface 配列の要素ごと `as?`(Box<i64> だけ 2 個カウント)、 sibling interface への `as?`(`class Multi: A, X` で `a as? X` が some)、 `as?` の hit/miss churn ARC(heap フィールド付き generic instance が delta=0・deinit 厳密・`ILANG_HEAP_GUARD=1` クリーン)が全て健全。 第 124 弾の `implements` が monomorphize された generic クラスにも正しく設定され、 instantiation ごとに区別されることを確認。 第 116 弾 fixture は `s as? Box<i64>`(成功)のみで、 型引数違いの失敗・instance→interface 方向は未 pin だった。 fixture 追加のみのため重い儀式は省略(直前の第 124 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `09_subtyping/generic_instance_is_as_discrimination.il`。
- **第 124 弾**。 interface 継承の継ぎ目を **`is`/`as?` ダウンキャスト**(dispatch でなく型テスト経路)で probe して **クラスが第一基底以外で実装する interface に対する `is`/`as?` が false/none を返す**バグを検出・修正。 `class Impl: C`(`interface C: B`, `B: A`)で `im is C` は true なのに `im is B`/`im is A` が **false**(継承した祖先 interface)、 `class Impl: A, X` で `im is X` も **false**(`interfaces` リストの 2 つ目以降)、 対応する `as?` も none。 原因: runtime 型テスト `emit_is_subclass`([mod.rs](../crates/ilang-mir-codegen/src/compile/mod.rs))が accept 集合を **単一の `parent` フィールドのみ**辿って構築していた。 クラスは**最初の** interface 基底だけを `parent` に記録する(`interface C: B` の継承は別の `iface_parents` に、 2 つ目以降の interface は `interfaces` リストにあり、 どちらも `parent` に乗らない)ため、 最初の 1 つしか認識されなかった。 修正: ClassLayout に推移的 interface 集合 `implements: Vec<ClassId>` を追加([program.rs](../crates/ilang-mir/src/program.rs))、 class lowering で既に祖先まで展開済みの `declared_ifaces`(第 120 弾と同じ push_iface_chain 由来)を class-id に変換して格納([class.rs](../crates/ilang-mir/src/lower/decl/class.rs))、 `emit_is_subclass` の accept スキャンを `parent` エッジに加えて `implements` エッジも辿るよう拡張(クラス祖先の interface は `parent` チェーンで合流)。 推移 interface 継承(C/B/A 全 true)・複数 interface(A/X 両 true)・`as?` の some/メソッド dispatch・否定(未実装 interface は false)・兄弟クラス(`Dog is Cat`=false)・クラス継承の非回帰を確認。 **新規/既存**: 複数 interface 実装は 2026-06-07(`multiple_interface_impl.il`)から存在し interface 継承より前 = `interfaces` リスト分は**既存バグ**、 interface 継承の推移分は新機能(7ef5d067)の実装漏れ。 fixture: `09_subtyping/interface_is_as_additional_and_inherited.il`。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100/100。 詳細は下の解決済み記録。
- **第 123 弾** (クリーンラウンド)。 第 120 弾で変更した vtable スロット割り当てループの周辺を **property アクセサ**で sweep — **新規バグなし**。 property の getter/setter は合成 MethodDecl(`name::get`/`name::set`)として同じスロットループでスロットを得るため、 第 120 弾の「サブクラスが class slot と interface slot を両方継承する」変更が property 仮想ディスパッチを壊していないことを確認。 interface を実装するクラス + 仮想 property(getter/setter)+ サブクラスの getter override を **親クラス型レシーバ**で呼ぶ形 — interface メソッド override(`b.tag()`=9)・スカラ getter override(`b.scalar`=1005)・heap getter override(`b.box.n`、 借用)・継承 setter への fresh heap 引数(旧 box が deinit・新 box は field 所有でリークなし)が全て健全。 churn で heap getter(借用)+ setter(fresh 引数)が delta=0・deinit 厳密・`ILANG_HEAP_GUARD=1` クリーン(過剰解放なし)。 第 103/104 弾の property fixture は interface 無し、 第 120 弾は property 無しで、 その交差が未 pin だった。 fixture 追加のみのため重い儀式は省略(直前の第 120 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `09_subtyping/property_interface_subclass_dispatch.il`。
- **第 122 弾** (クリーンラウンド)。 第 119/120 弾の interface dispatch ARC の周辺を別の値種別で sweep — **新規バグなし**。 closure の capture/escape/共有(heap capture が脱出して返る・closure 配列・2 closure が同一 capture を共有・`this` capture・field 格納 closure・closure 再代入・field 格納 closure の churn)、 **fresh な interface 型レシーバが heap を返す連鎖**(`getFactory().make().n` — 中間 interface 受信側と heap 返却の双方を解放)、 nested Optional of heap(`Box??`)を Optional 返却ヘルパー経由で unwrap、 が全て値正・churn delta=0・deinit 厳密で健全。 closure の churn delta=56 は §4-1 の罠(`acc` を計測開始後に確保)で、 acc を計測前に移し反復 100/200 で delta=0 を確認(線形リークでない)。 interface のメソッドオーバーロード(同名・異シグネチャ)は checker が「declares method more than once」で拒否(衝突リスクなし、 第 120 弾の name-keyed 衝突と同型の穴は無い)。 唯一未 pin だった **fresh interface 受信側 + heap 返却連鎖**(第 119 弾 fixture は fresh 引数のみ pin)と nested Optional を fixture 化。 fixture 追加のみのため重い儀式は省略(直前の第 120 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `09_subtyping/interface_chain_fresh_receiver_arc.il`。
- **第 121 弾** (クリーンラウンド)。 第 119/120 弾で触った interface vtable スロットの周辺を「サブクラス × 親クラス型レシーバ」で網羅 sweep — **新規バグなし**。 複数 interface 実装(`class Base: A, B`)+ サブクラス override、 同名メソッドを 2 つの interface が宣言(`A.ping`/`B.ping`)、 interface 裏付けクラスの override からの `super.foo()`(親クラス型レシーバ経由)、 サブクラスが親の実装しない新 interface を追加宣言(`class Derived: Base, X`)、 孫が親の override を再宣言せず継承(`Leaf: Mid: Base`)、 部分 override(一方の interface メソッドのみ override・兄弟は継承)が、 親クラス型レシーバ・interface 型レシーバの双方で全て正しくディスパッチすることを確認(第 120 弾 fixture が未カバーの組合せ)。 fixture 追加のみのため重い儀式は省略(直前の第 120 弾で全儀式を通し以降コード変更なし、 JIT + 個別 AOT で確認)。 fixture: `09_subtyping/interface_subclass_dispatch_combos.il`。
- **第 120 弾**。 第 119 弾に続き interface 継承の継ぎ目を「interface 実装クラス × サブクラス × 親クラス型レシーバ」で probe して **interface を実装するクラスのサブクラスを親クラス型変数で受け、その interface メソッドを呼ぶと SIGSEGV**するバグを検出・修正(既存バグ、 interface 継承コミット 7ef5d067 より前から存在)。 `class Base: A { foo(){..} }; class Derived: Base {}; let b: Base = new Derived(); b.foo()` がクラッシュ(非 interface メソッド `b.other()` や interface 型レシーバ `let a: A = new Derived(); a.foo()` は正常 = スロット違いで偶然動き隠れていた)。 原因: interface を実装するクラスは各 interface メソッドを **2 つの vtable スロット**に登録する — クラススロット(例 0)と interface の高位スロット(`1<<20`)。 サブクラスの継承スロット表 `parent_slots`([class.rs](../crates/ilang-mir/src/lower/decl/class.rs))が **name→単一 slot の HashMap** で、後から append される高位 interface スロットがクラススロットを上書きし、 サブクラスはクラススロットのエントリを完全に失う。 親クラス型からの呼び出しはクラススロットを引くため空スロットの garbage ポインタを deref。 修正: `parent_slots` を **name→Vec<slot>** にして両スロットを保持、 各メソッドにはクラススロット(`< IFACE_SLOT_BASE`)を割り当て、 継承した interface スロットは(override 後の)実装を指す MethodDecl として後段で再登録、 新メソッドの `next_slot` はクラス範囲のみから計算(interface スロットで膨らませない)、 親継承 + 自身再宣言の二重到達に備え `(name, slot)` で dedup。 これで Derived が Base と同じく両スロットを持つ。 単純な単一 interface・interface 継承(B:A)・多段(GrandChild)・override(クラス型/ interface 型双方で override 命中)・新メソッド追加・親クラス型配列の動的ディスパッチ・heap メソッドの ARC(churn delta=0・HEAP_GUARD クリーン)を確認。 fixture: `09_subtyping/interface_method_base_typed_receiver.il`。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100/100。 詳細は下の解決済み記録。
- **第 119 弾**。 直近の interface 継承(7ef5d067)の継ぎ目を probe して **仮想ディスパッチ呼び出しに fresh heap 引数を渡すと引数がリーク**するバグを検出・修正(継承以前からの既存バグ、 interface dispatch が踏まれやすくなり顕在化)。 `iface.method(new Box())`(interface 経由 VirtCall)と `obj.fnField(new Box())`(closure フィールド呼び出し)で fresh 引数が呼び出し毎に 1 個リーク(deinit が 1 個不足、 churn で線形増加)。 concrete 受信側の直接呼び出し(`lower_class_method_call`)は `fresh_obj_args` で fresh 引数の transient +1 を呼び出し後に Release していたが、 `lower_iface_dispatch` と `lower_fn_field_call`([object.rs](../crates/ilang-mir/src/lower/calls/object.rs))は引数を lower するだけで解放経路が無かった。 両関数に直接パスと同じ fresh-arg 解放(`is_fresh_object_expr ∨ last_arg_wrapped` かつ `fresh_arg_needs_post_release`)を移植。 継承スロット経由(child-interface 受信側で親メソッド呼び出し)・field 格納する method(過剰解放なし)・borrowed 変数引数(解放しない)を確認、 HEAP_GUARD クリーン。 free 関数・closure 変数呼び出しは元から健全。 同型の C ABI 経路(`lower_com_iface_dispatch`・c_vtable)は外部メソッドで ilang heap を渡す経路が稀かつ所有権規約が異なるため対象外(ilang で deinit 観測できる経路のみ修正)。 fixture: `09_subtyping/interface_dispatch_fresh_arg_release.il`。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100/100。 詳細は下の解決済み記録。
- **第 105 弾**。 オブジェクト/enum の等価比較を probe して **ペイロード付き enum の `==`/`!=` が判別子のみ比較しペイロードを無視する**バグを検出・修正(user 承認の上で構造的比較を実装)。 `circle(5) == circle(9)` が true、 `str("hi") == str("yo")` も true(数値・複数・文字列・heap すべて無視)。 user 判断で**構造的比較**(本来の形)を選択。 実装は runtime ヘルパー方式: `__enum_structural_eq(a, b)`([enums.rs](../crates/ilang-runtime/src/enums.rs))が判別子一致時に既存の `enum_payload_kinds` メタデータ(`(eid,tag)→Vec<KIND>`)でスロットごとに比較 — 数値/bool(KIND_NONE)=ビット比較、 文字列=`__str_eq`、 入れ子 enum=再帰(再帰 enum も関数呼び出しなので停止)、 オブジェクト/他 heap=参照比較(`Object ==` と同じ規約)。 generic enum は monomorphize 後の eid/kind を runtime が読むだけで自動対応。 副次修正: payload kind 登録([registration.rs](../crates/ilang-mir-codegen/src/compile/registration.rs))が heap スロットのみ登録していたのを**全スロット登録**に(数値のみ variant がエントリ無し=ペイロードレス扱いになるのを是正、 release 経路は 0 を skip するので無害)。 lowering([ops.rs](../crates/ilang-mir/src/lower/ops.rs))は enum Eq→helper 呼び出し、 Ne→否定、 ordering(`<`等)は repr enum の tag-only を維持。 builtin 配線(mod.rs struct・program_decl 宣言・jit_symbols 登録・calls.rs match)。 数値/複数/文字列/入れ子/オブジェクト/unit/別 variant・@flags/repr enum 非回帰を確認。 Object `==` は参照等価(docs 233 行)、 Optional `==` は checker 未対応(別件)。 fixture: `06_enums/enum_structural_equality.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 106 弾**。 第 105 弾の構造的 enum 等価の周辺を probe して **enum `==` の fresh オペランド(heap cell + heap payload)が解放されずリークする**バグを検出・修正(旧 tag-compare からの既存リーク、 構造的比較で heap-payload enum が比較されやすくなり顕在化)。 `E.holds(new Obj()) == E.holds(new Obj())` で Obj が deinit されない(deinit 0/3)。 原因: lower_binary の enum 比較は早期 return し、 fresh オペランド解放(文字列のみ存在)を通らなかった。 enum Eq/Ne/ordering の return 前に `lhs_fresh`/`rhs_fresh` の Release を追加(比較は借用のみ)。 @flags bitwise は interned unit のため per-op リークなし(8000 回で 64 バイト一定)を確認。 Optional/Result の `==` は checker 未対応(クリーンな型エラー = 機能ギャップ、 crash でない、 対象外)。 fixture `06_enums/enum_structural_equality.il` に fresh-operand ARC 検査(deinit tally)を追記。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 110 弾**。 第 107–109 弾で残した最後の generic メソッドギャップ **implicit-this(`twice<U> { this.apply(f) }` のように generic メソッドが `this` 経由で別の generic メソッドを呼ぶ)が「no method apply<string> on class」で失敗**を修正(user 承認)。 原因: checker は `this` 呼び出しを generic 基底名(`Box`)で記録する(check 時クラスは generic)ため、 method 単一化が specialized `Box<i64>` に対応付けられない。 修正([methods.rs](../crates/ilang-mir/src/monomorphize/methods.rs)): (1) seed/rewrite 系に「囲む specialized クラス」`enclosing` を thread し、 記録 class がその generic 基底(`<` 前)と一致したら specialized 名へ remap する `self_class` を追加、 (2) specialized body の **seed を rewrite より先に**実行(rewrite が inner call を mangled 名へ改名すると seed の `recorded_method == method` が外れるため)。 単一 this 呼び出し・多段(thrice = this→twice→apply)・this 呼び出し + generic 返却(`new Box<U>(this.apply(f))`)を確認。 これで generic メソッドの主要ギャップ(外部呼び出し・generic 返却・連鎖・implicit-this)が全て解消。 残るは明示メソッド型引数 `obj.method<T>(...)`(parser 未対応、 推論で代替)のみ。 fixture: `05_edge_cases/generic_method_this_call.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 112 弾** (クリーンラウンド)。 デフォルト引数と @derive(Eq, Hash) を sweep — **新規バグなし**。 デフォルト引数(値補完・評価順・heap デフォルト値の ARC・**毎回再評価**・メソッド/init のデフォルト・オーバーロード解決での exact-match 優先)、 @derive(Eq, Hash)(i64/string/float/ネストクラスフィールドの equals/hashCode、 Set dedup・Map キー)が全て健全(float の hashCode 合成は動作 — 古い「floats は失敗」注記は無効、 `class_derive_float_field.il` で pin 済み)。 唯一未 pin だった **heap デフォルト値の ARC**(per-call の default Box が漏れず deinit)と **デフォルト式の毎回再評価**(omit 毎に再実行、 明示引数では非実行)を fixture 化。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `01_basics/default_arg_heap_and_reeval.il`。
- **第 113 弾** (クリーンラウンド)。 for-in/range・オーバーロード解決・match・文字列を広く sweep — **新規バグなし**。 for-in(配列・break/continue・ネスト・range・heap ARC・break で heap 持ち出し・反復中 push は live 反復・Set/Map の values/keys/entries 経由)、 range(排他 `[lo,hi)`・空・lo>hi で空・負・式境界)、 オーバーロード解決(サブクラス距離・widening・arity・array/scalar・数値リテラルは i64 既定で曖昧性なし)、 match(ペイロード束縛・2 heap payload の node・値返却・heap ARC・wildcard — 第 105 弾の構造的 eq の影響なし)、 文字列(空・unicode code-point 長・slice clamp・ZWJ escape)が全て健全。 ユーザー定義 variadic(`...xs`)は parser 未対応(機能ギャップ)、 ZWJ リテラルは lexer が trojan-source 対策で拒否(`trojan_source_rejected.il` で pin 済み)。 これらの領域は既存 fixture で厚く網羅済み。 唯一未 pin だった **文字列の順序比較が型エラー**(`<`/`<=`/`>`/`>=` は不可、 docs 233 行)を `expect-error` fixture 化。 fixture: `05_edge_cases/string_ordering_error.il`。
- **第 115 弾**。 第 114 弾に続き generic × subtyping を probe して **if/match の分岐 join で generic クラス instantiation のアームが共通 interface に join されない**バグを検出・修正。 `let s: Sized = if c { new Box<i64>(..) } else { new Empty() }`(両方 Sized 実装)が「expected Box<i64>, got Empty」、 混在・異インスタンス化(Box<i64> vs Box<string>)・3 アーム match も全滅(非 generic の Dog/Cat join は動作)。 原因: branch join([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs) の check_if_expr、 [utils.rs](../crates/ilang-types/src/checker/utils.rs) の unify_branch_obj)の共通祖先探索が `(Type::Object, Type::Object)` のみで `Type::Generic` を素通り。 修正: (1) どちらかが `Type::Generic` のとき base クラス名へ正規化して `common_object_join`(covariant join の後に配置し `Box<Dog>⊔Box<Cat>→Box<Animal>` を非回帰)、 (2) `common_object_join` に「片方が他方の実装する interface ならその interface が join」を追加(3+ アーム fold で arm0⊔arm1 が interface になった後の合成に必要)。 generic+非generic 混在 if、 異インスタンス化 if、 generic+generic+非generic の 3 アーム match、 非 generic 回帰、 covariant join 非回帰を確認。 generic クラスのクラス継承は明示的に未対応(clean error)。 fixture: `09_subtyping/generic_instance_branch_join.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 116 弾** (クリーンラウンド)。 第 114/115 弾の generic × subtyping 修正の周辺を sweep — **新規バグなし**。 generic instance → interface の covariance が全位置で健全: covariant return(`fn make(): Sized { new Box<i64>(..) }`)・`Optional<Sized>`・`Map<string,Sized>` 値・tuple 要素・array(generic+非generic 混在)、 `is Sized`/`as? Empty`/interface→generic instantiation のダウンキャスト(`s as? Box<i64>`)も全て正常。 唯一未 pin だった **composite 位置(Optional/Map/tuple/return)+ ダウンキャスト** の generic→interface covariance を fixture 化(round-114 fixture は param/local/array のみ)。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `09_subtyping/generic_instance_interface_covariance.il`。
- **第 118 弾** (クリーンラウンド)。 第 117 弾の static フィールド継承の周辺を sweep — **新規バグなし**。 static メソッドのサブクラス継承(`Mid.who()`→`"base"`)・shadowing(`Leaf.who()`→`"leaf"`)・static→static 呼び出し + 継承 static フィールド読み(`Mid.useHelper()`=142)が全て健全。 generic クラスの static メンバーは明示的に未対応(clean error、 機能ギャップ)。 唯一未 pin だった **static メソッド継承/shadowing + static→static + 継承 static フィールド** を fixture 化(`static_methods.il` は単一クラスの factory static のみ)。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `02_classes/static_method_inherited.il`。
- **第 117 弾**。 static メンバーを probe して **static フィールドだけがサブクラス名で継承されない**バグを検出・修正(static メソッド/getter は `Derived.make()`/`Derived.version` で継承されるのに、 static フィールド `Derived.count` が「undefined variable Derived」(checker)/「unbound variable」(MIR))。 static フィールドは単一共有スロットなので `Derived.count` は `Base.count` のエイリアスであるべき。 原因: checker の `static_fields`/`static_field_pub`/`static_const_fields`([sigs.rs](../crates/ilang-types/src/checker/sigs.rs))が親から seed されず空から始まる(properties は seed 済み)、 MIR の `meta.static_slots`([class.rs](../crates/ilang-mir/src/lower/decl/class.rs))も自クラス分のみ。 両方を親から継承するよう修正(checker は型を、 MIR は同一スロット id を継承し共有スロットを実現)。 サブクラス/孫からの read/write が同一スロットを共有、 継承 const は read 可・再代入は clean error、 多段継承も確認。 fixture: `02_classes/static_field_inherited.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 114 弾**。 generic × interface を probe して **generic クラスが interface を宣言(`class Box<T>: Sized`)してもその instantiation `Box<i64>` が interface のサブタイプと認識されない**バグを検出・修正。 `box.size()` は直接動くのに `describe(box)`(`Sized` 引数)や `let s: Sized = box` が「type mismatch: expected Sized, got Box<i64>」。 非 generic クラスの interface 適合は動作。 原因: `literal_assignable_with`([mod.rs](../crates/ilang-types/src/checker/mod.rs))の interface 適合チェックが `from` を `Type::Object` からしか抽出せず、 generic instantiation の `Type::Generic` を素通り。 適合関係は base クラス(`Box`)に宿るため、 `Type::Generic(g)` の場合 `g.base` を適合候補にするよう拡張(builtin generic/Map/Set 等は class_implements が false なので誤適合なし)。 generic instance の interface 引数/ローカル代入、 混在(generic Box/Pair + 非 generic Empty)interface 配列の**動的ディスパッチ**(1+2+0=3)を確認。 closure(ループ内値キャプチャ・heap キャプチャ ARC)、 repr enum int 変換・範囲外 panic、 generic class + interface 以外は健全。 fixture: `09_subtyping/generic_class_implements_interface.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 111 弾** (クリーンラウンド)。 ダウンキャスト・Optional・ビット演算・const を広く sweep — **新規バグなし**。 `is`/`as?`(具象の多段継承・skip-level・exact 型・interface 経由・成功束縛/失敗/fresh-receiver の ARC)、 ネスト Optional(`i64??`)・Optional<Box>・Optional<Box[]>・none・unwrap の ARC、 ビット演算(`& | ^ ~`・signed 算術右シフト `-8>>1=-4`・u64 論理右シフト・幅以上シフト・変数シフト)、 const 畳み込み(const 参照 const・cast・hex/shift/neg・大算術)が全て健全。 唯一未 pin だった **右シフトの符号別意味論**(signed=算術 / unsigned=論理、 `bitwise.il` は一般的な `>>`/`<<` のみ)を fixture 化。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `01_basics/shift_signedness.il`。
- **第 109 弾**。 第 107/108 弾で残した **連鎖 generic メソッド呼び出し(`b.remap(f).remap(g).remap(h)`)が「no method remap on class」で MIR lower 失敗**するバグを修正(user 承認、 固定点単一化を実装)。 原因: 各リンクの結果クラス(`Box<string>`→`Box<i64>`→`Box<bool>`)は前リンクのメソッド特殊化後に初めて合成されるため、 class/method 単一化の単発実行では2リンク目以降が特殊化されない。 修正: pipeline([main.rs](../crates/ilang-cli/src/main.rs))を **class mono + method 単一化をテンプレート再アタッチ込みで収束まで反復**する固定点ループに(具象クラス名+メソッド名の構造的 fingerprint が安定で停止、 64 回上限・class-mono の recursion limit が無限増殖を別途防止)。 併せて `rewrite_item` の Class 処理([class.rs](../crates/ilang-mir/src/monomorphize/class.rs))が generic メソッドを un-rewritten のまま残すよう変更(Fn のスキップと同様、 早すぎる `Box<U>` mangle を防止)。 2/3 段連鎖・単一 remap・apply・nested generic を確認。 **残る既知の制限**: **implicit-this** の generic メソッド呼び出し(`this.apply(f)`)は checker が `this` 呼び出しを generic 基底名で記録するため未解決(別機構、 per-specialization rewrite に specialized クラス文脈を渡す追加修正が必要)、 明示メソッド型引数。 fixture: `05_edge_cases/generic_method_chain.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 108 弾**。 第 107 弾で残した **generic 返却メソッド(`remap<U>: Box<U>` が `new Box<U>` を返す)が MIR lower で「unknown type: Box<U>」** になるバグを修正(user 承認、 map/transform パターン)。 原因: class 単一化が still-generic メソッドへ class param T を置換する際、 メソッド自身の param U が `Object("U")` で到着し、 `subst_type` の mangle 判定の `contains_type_var` が `Object("U")` を type var と認識しないため `Box<U>` を phantom `Object("Box<U>")` に焼き込む。 method 単一化は型を置換するが New の class Symbol を直せず U が残存。 修正([class.rs](../crates/ilang-mir/src/monomorphize/class.rs)): (1) `specialize_class` が generic メソッドを **メソッド自身の型 param を `TypeVar` へ写す substitution で再 specialize**(`specialize_generic_method`)— `Box<U>` を un-mangled に保ち、 後段の `specialize_fn`(U→concrete)で正しく mangle、 (2) instantiation 収集の scan_fn ループで generic メソッドをスキップ(未置換 param 由来の phantom 合成を防止)。 Box<i64>→Box<string>・Box<i64>→Box<i64>・別 receiver(Box<string>→Box<i64>)・heap ARC を確認。 **残る既知の制限**(より深い、 第 107 弾記載): implicit-this の generic メソッド呼び出し、 連鎖 `b.remap(f).remap(g)`(中間値 receiver の mangle 漏れ)、 明示メソッド型引数。 fixture: `05_edge_cases/generic_method_returns_generic.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 107 弾**。 generic を probe して **generic クラス上の generic メソッド(メソッド独自の型パラメータ持ち)が推論呼び出しで MIR lower 失敗**するバグを検出・主ケースを修正(user 承認)。 `class Box<T> { apply<U>(f: fn(T):U): U {...} }` で `box.apply(fn...)` が「no method apply<string> on class」。 原因: class 単一化が `Box<T>` を mangled 名 `Box<i64>` へ rename した後に method 単一化が走るのに、 checker が呼び出しサイトを bare 名 `Box` で記録していたため、 `monomorphize_methods` の generic_methods 索引(specialized 名)と一致せず synthesize されず call が dangling。 修正: checker の `resolve_method_call`([method.rs](../crates/ilang-types/src/checker/method.rs))に receiver のクラス型引数 `recv_class_args` を渡し、 generic instantiation の場合は `method_call_type_args` を **mangled 名**(`mangle_generic_class_name`、 ilang-mir の `mangle` と byte 一致)で記録([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs) の 5 caller を更新、 generic instance caller のみ `&inst_args`)。 外部呼び出し・複数インスタンス化(Box<i64>/Box<string>、 同一 receiver で U 違い)・heap ARC を確認。 **残る既知の制限を記録**(より深い単一化ギャップ、 いずれも本修正前から壊れていた): (1) **implicit-this** の generic メソッド呼び出し(`this.apply(f)`)は in_class 名が bare のため未解決、 (2) **generic 返却**メソッド(`remap<U>: Box<U>` が `new Box<U>` を返す)は specialized body の U 未置換/二巡目 class 合成の連携で「unknown type: U」、 (3) **明示メソッド型引数** `obj.method<T>(...)` は parser 未対応(docs の `then<U>` はシグネチャ表記、 推論で代替)。 fixture: `05_edge_cases/generic_method_on_generic_class.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 103 弾**。 プロパティ(get/set)を probe して **プロパティ getter/setter が仮想ディスパッチされない**実バグを検出・修正(user 承認の上で実装)。 `let a: Animal = new Dog(); a.sound` が override した Dog の getter でなく基底 Animal の getter を返す(setter も同様に override を無視)。 通常メソッドは vtable slot 経由で `VirtCall` だが、 プロパティアクセサは受信側の**静的型**から解決した `FuncRef::Local` で直接呼ばれ vtable に載っていなかった(getter override に `override` 不要だったのもその表れ)。 修正: class lowering([class.rs](../crates/ilang-mir/src/lower/decl/class.rs))でプロパティ登録時に合成名 `name::get`/`name::set` の `MethodDecl` を `method_decls` に push(override は既存を retain で除去)— これで既存の slot 割り当てループ・vtable 構築・親 slot 継承が自動で乗る。 dispatch 側(getter = [literals.rs](../crates/ilang-mir/src/lower/literals.rs)、 setter = [expr.rs](../crates/ilang-mir/src/lower/expr.rs))を slot ありなら `VirtCall`、 なければ従来 `FuncRef::Local` にフォールバック。 getter/setter override・多段継承(Puppy が Dog の override を継承)・非 override 継承プロパティ・heap getter/setter ARC(VirtCall 経由でもリーク/UAF なし)・静的型直接アクセスを確認。 既存 `lower_e2e` テスト `class_property_get_set`(旧 `call func#` を期待)を `virt_call` 期待に更新。 fixture: `09_subtyping/property_virtual_dispatch.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 104 弾**。 第 103 弾の仮想プロパティ修正に隣接して probe し、 **サブクラスが片方のアクセサだけ override すると継承したもう片方が失われる**(getter のみ override → setter が消え read-only、 setter のみ → write-only)checker バグを検出・修正(round-103 とは独立、 checker 側)。 原因: [sigs.rs](../crates/ilang-types/src/checker/sigs.rs) が子の `properties` を親から seed するのに、 各プロパティの `has_get`/`has_set` を**子の宣言のみ**から決めて上書きしていた。 子が getter だけ宣言すると `has_set=false` で上書きされ親の setter が見えなくなる。 アクセサの有無を継承エントリとマージ(`has_get = 子に getter ∨ 継承の has_get`、 setter も同様)するよう修正。 MIR 側は round-103 で親の `property_getter`/`setter` と vtable slot を継承済みのため、 checker が許可すれば inherited 方向は親アクセサへ正しく VirtCall。 getter-only/setter-only override の両方向を base ref 経由の仮想ディスパッチ + 直接アクセスで確認。 インターフェースのプロパティ宣言・`super.prop` アクセスは parser 未サポート(機能ギャップ、 crash でない、 対象外)、 generic クラスのプロパティ・static プロパティ(非仮想)は健全と確認。 fixture: `09_subtyping/property_partial_override.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 102 弾** (クリーンラウンド)。 第 101 弾に続き collection ARC と周辺を広く sweep — **新規バグなし**。 配列 `indexOf`/`includes`/`remove`・`Set` `has`/`delete`/`add` の fresh heap 引数解放(第 101 弾と同類型の借用ルックアップ — 全て健全、 既存 `map_set_fresh_needle_arg_release.il` で pin 済み)、 tuple の分解代入/fresh-receiver index/ネスト、 Map の `keys`/`values`/`entries`/`forEach` のオブジェクト ARC、 配列 `map().filter()` 連鎖(中間配列解放・filter 除外 heap 要素の deinit)、 が全て健全。 template literal は各型・ネスト・空・リテラル波括弧が正確(`${obj}` がユーザー `toString()` でなく `console.log` 同等の構造フォーマットを使うのは docs 285 行明記の仕様)。 唯一未 pin だった **`Set<f64>` の特殊値ビット列セマンティクス**(`has(nan)` が NaN!=NaN を回避して true・`0.0`/`-0.0` が別エントリ・±Inf が別エントリ・NaN の bit-pattern delete)を fixture 化(`set_element_types.il` は通常 float dedup のみ)。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `03_collections/set_float_bit_pattern.il`。
- **第 101 弾**。 第 100 弾の async ARC 修正に隣接して collection の ARC を probe し、 **Map をオブジェクトキーで index 読み取り(`m[new K(1)]`)すると lookup キーがリークする**実バグを検出・修正。 `__map_get` はキーを**借用**するだけ(hash + equals、 所有権を取らない)だが、 index 読み取りの lowering([literals.rs](../crates/ilang-mir/src/lower/literals.rs))が set/has/delete 経路と違い fresh heap キーの transient +1 を解放していなかったため、 読み取り 1 回につきキー 1 個リーク(`m[a + b]` の fresh 文字列キーも同症状)。 値(V)は正しく解放されており**キーのみ**漏れていた(切り分け: read だけ 2 作成 1 deinit、 overwrite/has/delete は 2/2)。 `MapGet` 後に `key_is_fresh && is_arc_heap(key_ty)` で fresh キーを Release(set 経路と同じ規約)。 変数キー(非fresh)は借用のままで過剰解放なし、 文字列キー(literal/fresh/fresh-receiver)も健全。 async 隣接面(async 早期 return の heap・Promise.all/race の heap)は既存 fixture + 追加 probe で全て健全と確認。 fixture: `03_collections/map_index_read_object_key_release.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 100 弾**。 async/await を probe して **suspend する async fn が heap 値を返すとその返り値がリークする**実バグを検出・修正。 `async fn r(p): Box { let n = await p; new Box(n) }` を呼ぶと返した Box が永久に未解放(呼び出し毎に 1 個)。 await を含む(state machine 経由の)場合のみ発生し、 await なし async fn(`Promise.resolve(body)` に desugar)は健全。 切り分け: promise/state 構造は回収されるが heap 返り値のみ漏れる(promise は Rust の `Box::new` で `liveAllocBytes` 非対象、 漏れた `__async_promise` が保持する ilang heap 値だけが可視)。 runtime トレース(obj retain/release + `ILANG_DEBUG_PROMISE`)で `__async_promise` が rc 1 のまま FINAL-DROP しないことを特定。 原因は `lower_promise_settle_resolve`([builtin_static.rs](../crates/ilang-mir/src/lower/calls/builtin_static.rs)): desugar が `settleResolve(state_ref.__async_promise, value)` を発行する際、 第1引数の promise(フィールドアクセス=非fresh)を **Retain** していたが、 runtime の `settle_resolve` は promise を**借用**するだけで所有権を取らない(lock→状態遷移→継続を queue するのみ、 内部の chain 呼び出しは pre-retain なし)ため、 この +1 が未解放のまま promise をリークさせていた。 promise 引数の retain を両 settle 経路(resolve / reject)から削除(value/msg の retain は settle が所有権を取るため維持)。 await rejection 伝播・複数 await・await 跨ぎの heap local 保持も健全、 リークは呼び出し数に依らず一定(=解消)。 fixture: `04_modules/async_heap_return_release.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 99 弾** (クリーンラウンド)。 weak 参照のライフサイクルを sweep — **新規バグなし**。 fresh オブジェクトを直接 weak へ束縛すると即 deinit(strong 不在)、 strong+weak で `.get()` が Some/None を正しく返す、 複数 weak が同時に死を観測、 weak 再代入を 10000 回 churn してもリーク 0、 weak を配列に格納して `.get()`、 そして **`.get()` のアップグレードが他の全 strong を落としても binding 生存中はオブジェクトを保持し binding 終了で 1 回だけ deinit(UAF/二重解放なし)** が全て健全。 既存 fixture(weak_basic = Some/None、 leak_weak_get_loop = upgrade 毎の reclaim)は upgrade の **use-after-free 防止保証**(upgrade した binding が唯一の所有者になるケース)を pin していなかったため、 tally で deinit タイミングを検証する fixture を追加。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `05_edge_cases/weak_upgrade_outlives_last_strong.il`。
- **第 98 弾** (挙動を明文化・コード変更なし)。 整数演算の UB ケースを probe して **符号付き狭整数(i8/i16/i32)のローカル算術が宣言幅へ wrap しない**不整合を検出。 `let a: i32 = 2e9; let b: i32 = 5e8; let c: i32 = a + b` が i32 範囲外の `2500000000` を保持し `c < 0` も false(符号なし u8/u16/u32 は幅で wrap、 配列セル/フィールド格納・i64 narrowing キャストも wrap、 **符号付き狭整数のローカル算術だけが i64 幅のまま**)。 `i64::MIN / -1` は trap せず i64::MIN へ wrap、 負数の剰余・除算の符号は健全。  user 判断で **現状維持・docs 明文化**を選択(算術への wrap 挿入は codegen の算術経路に広く手を入れる中規模変更で、 トリガーも narrow-typed オペランド同士の overflow 限定)。 [syntax.md](syntax.md) の「Integer overflow」節に wrap 幅の規則(オペランド型で決まる・bare リテラルは i64・wrap は narrowing キャスト/暗黙 narrowing 代入/狭セル格納の境界で起きる・`(a+b) as i32` は同型なら no-op)を正確に記載し、 回帰防止 fixture を追加。 fixture: `05_edge_cases/int_signed_narrow_overflow.il`。
- **第 97 弾** (クリーンラウンド)。 f32・文字列メソッド・Set 集合演算を sweep — **新規バグなし**。 f32 の算術・特殊値・**f32→狭整数の飽和**(第 96 弾の修正が f32 源でも有効)・f32↔f64 変換・int→f32 の精度丸めが健全(f32 の `toString` が f64 へ昇格して整形するのはコメント明記の意図的設計 — `console.log`/テンプレートと一致させるため、 バグでない)、 文字列 `split`/`indexOf`/`lastIndexOf`/`charAt`/`slice`/unicode が JS 整合で正確(`replace` の全置換は docs 223 行「Rust-style」で意図通り)、 Set の `union`/`intersection`/`difference` のサイズ・membership が健全。 既存 `set_of_class.il` は集合演算のサイズ/membership のみ検証で **ARC が未確認**だったため、 **オブジェクト要素の集合演算 ARC**(結果集合が要素参照を共有する — value-equal な重複・共有要素が過不足なく 1 回ずつ deinit)を deinit tally + `liveAllocBytes` リーク検査で pin。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `03_collections/set_algebra_object_arc.il`。
- **第 96 弾**。 浮動小数点→整数キャストを probe して **float → 狭い整数型(i8/i16/u8/u16)が飽和でなくラップする**バグを検出・修正。 `300.0 as u8` が 255 でなく **44**、 `40000.0 as i16` が 32767 でなく **-25536**、 `-200.0 as i8` が -128 でなく **56**(i32/i64/u32/u64 は正しく飽和)。 原因は FloatToInt の codegen([binop_cast.rs](../crates/ilang-mir-codegen/src/compile/binop_cast.rs)): x64 backend は飽和 `fcvt_to_*_sat` の宛先を I32/I64 しか許さないため狭い型は一旦 I32 に変換するが、 **その後の `ireduce` が低ビットを取って WRAP** していた(コメントは「I32 が範囲内だから ±32767 で飽和する」と書いていたが誤り — I32 飽和は I32 境界でしか効かない)。 I32 飽和後に**宛先型の範囲へ整数ドメインで再クランプ**(unsigned は `umin(x, 2^bits-1)`、 signed は `smax(smin(x, max), min)`)してから ireduce するよう修正。 NaN→0・±Inf→MAX/MIN・負値→0(unsigned)も全狭型で正しく飽和。 in-range・i32/i64/u32/u64 は無変更。 既存 `cast_float_to_int_saturating.il`(i64/i32/u32 のみカバー)に i8/i16/u8/u16 の飽和ケースを追加。 検証: nextest 539/539、 AOT 全 fixture PASS、 JIT/AOT 一致。 詳細は下の解決済み記録。
- **第 95 弾** (クリーンラウンド)。 再帰ヒープ構造・配列メソッド・インターフェース動的ディスパッチを sweep — **新規バグなし**。 連結リスト(200 万ノード)・cons-list・二分木(65535 ノード)の **deinit がスタックを溢れさせず全ノード 1 回ずつ発火**(ARC release は worklist で iterative)、 配列 `slice`/`pop`/`concat` の ARC(複数ホルダ越しに全要素過不足なく deinit)、 `slice` の端引数(負 start・range 外 end・start>end)は実装通り `start.max(0).min(len)` で**明示クランプ**([arrays.rs](../crates/ilang-runtime/src/arrays.rs):685)、 インターフェースの混在サブタイプ配列リテラル(`Shape[] = [new Sq, new Rect, new Sq]`)の covariance・仮想ディスパッチ・ARC、 が全て健全。 既存 `deep_release_iterative.il` は線形グラフ(`next` リスト・cons-list)のみだったため、 **各ノードが left/right の 2 heap 子を持つ二分木の iterative release**(worklist が 1 pop で複数の子を抱えて drain する分岐ケース)を deinit 数 + `liveAllocBytes` リーク検査付きで pin。 別件の設計上の制限を記録(バグでなく仕様): 配列 `slice` の負インデックスは JS 風の末尾ラップでなく 0 クランプ(文字列 slice の「out-of-range clamps」と整合)、 インターフェース適合は**名前的**(`class C: I` 明示が必要、 構造的適合は非対応)。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `05_edge_cases/deep_release_binary_tree.il`。
- **第 94 弾** (クリーンラウンド)。 第 93 弾の u64 幅修正に隣接して数値幅・FFI レイアウト・クロージャ ARC を広く sweep — **新規バグなし**。 u64/u32/u8/u16 の**符号なし比較・論理右シフト・除算/剰余・ゼロ拡張・wraparound**(2^63/2^31 超の値含む)、 @extern(C) union の type-punning(既存 fixture が u32↔f32↔i32↔u8[4]↔u64 を網羅)、 @packed の非整列 u64、 @flags ビット演算、 クロージャの heap キャプチャ(返却・配列共有・`this` キャプチャの単一 deinit)、 文字列メソッド連鎖 ARC、 Map<string,Object> の反復/キー上書き時の即時 deinit、 が全て健全。 唯一未 pin だった **u64 要素の FAM**(既存 `repr_c_flex_array.il` は u8/i32 のみ)を既存 fixture に追加して pin — 8 バイト stride を FAM 合成ヘッダ経由の ArrayLoad/Store(第 93 弾の bitfield 経路とは別)で通し、 全 64bit 往復・隣接スロット非干渉・固定領域との非重複を確認。 別件の設計上の制限を記録(バグでなく clean な拒否): **struct 要素の FAM**(`point[]` を最後のフィールドに)は checker が「type point[] not supported」で拒否(C では合法だが ilang は FAM 要素を primitive/bool/str/fixed-array に限定)。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `04_modules/repr_c_flex_array.il` に u64 FAM ケース追加。
- **第 93 弾**。 @bits bitfield の**読み書き codegen** を probe して **u64 の bitfield(幅 33〜64)が下位 32bit に切り詰められる**バグを検出・修正。 既存 fixture(`repr_c_bitfield.il`)は u32(≤32bit)と u8 のみで、 `@bits(64) b: u64` を全 1 で書くと `-1` でなく `0xFFFFFFFF`、 `@bits(33) lo: u64` が 32bit にマスク、 u64 パターンの上位 32bit が消失していた。 原因は bitfield read/write([objects.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/objects.rs))が storage 単位の CLIF 型を `elem_clif_type` 経由で決めていたこと: この helper は **I64/U64 に対して `None` を返す**(i64 セルは別経路の catch-all 設計)ため、 u64 フィールドが `_ => I32` の fallback に落ち、 ロード・シフト・マスク・read-modify-write store の全てが 32bit 幅で行われていた(store は下位 4 バイトしか書かないので上位は未更新)。 マスク値計算自体は既に 64bit 対応済み。 専用 helper `bitfield_storage_clif_type`([abi.rs](../crates/ilang-mir-codegen/src/compile/abi.rs))を追加し u8/u16/u32/u64 → I8/I16/I32/I64 を直接マップ、 read 側(field 型 `dst_ty_mir`)と write 側(値型でなく**フィールド宣言型**を参照するよう変更し read と一致させた)の双方をこれに差し替え。 u64 ユニットのパッキング独立性(隣接フィールド非破壊の RMW)・宣言幅での truncation も確認。 u32/u8 既存ケースは無変更。 fixture: `04_modules/repr_c_bitfield_u64.il`。 検証: nextest 539/539、 AOT 全 fixture PASS。 詳細は下の解決済み記録。
- **第 92 弾** (既知の制限として記録・コード変更なし)。 深い入れ子式を probe して **約 1000 階層以上の式でコンパイラが stack overflow し abort する**ロバスト性の穴を検出。 `1 + 1 + … `(左結合二項式 ~1000 項)・深い nested parens・長いメソッドチェーンが `thread 'main' has overflowed its stack`(rc=134)でクラッシュ(パーサは反復なので二項式自体は浅いが、 checker / MIR lowering が式 AST を**再帰的に walk** するため)。 配列リテラル(10000 要素)は反復処理で健全。 **標準的な修正(大スタックの worker thread で実行)を試したが撤回**: ilang は JIT 実行したユーザーコードがプロセス内で動き、 その中の **Cocoa/Foundation 呼び出しが macOS で main thread 必須**のため、 worker thread に移すと `cocoa_foundation` の `collections_test.il` が fail + timeout する。 残る選択肢(checker/lowering/const-fold の再帰 walk への depth-limit)は複数パスに手を入れる中規模変更で、 トリガーが adversarial / generated コード限定(手書きコードは非到達)のため、 **ユーザー判断で現状維持**(既知の制限として記録)とした。 コード変更・fixture 追加なし。
- **第 91 弾**。 generic 入れ子/再帰を probe して **再帰的 generic class で monomorphizer が無限ループしコンパイラがハングする**バグを検出・修正。 `class Wrap<T> { doubled(): Wrap<Wrap<T>> { ... } }` で `new Wrap<i64>(7)` を作るだけで(doubled を呼ばなくても)、 eager な全メソッド instantiation が `doubled()` の戻り型 `Wrap<Wrap<i64>>` → `Wrap<Wrap<Wrap<i64>>>` → … を無限に enqueue。 各レベルが distinct な mangled 名なので `synthesized` dedup が効かず worklist が drain しない。 class monomorphization の drain ループ([class.rs](../crates/ilang-mir/src/monomorphize/class.rs))に **count limit(1000 instantiation)** を追加し、 超過時に明確な panic(「monomorphization limit exceeded … recursively instantiated at ever-deeper type arguments」)で即時終了(Rust/C++ の recursion limit と同様。 `monomorphize` は `Program` を返す infallible 関数なので panic を選択)。 ハング→~1 秒の明確エラーに。 全 539 test は 1000 を超えず false-positive なし。 fixture: `05_edge_cases/mono_recursion_limit_error.il`。 詳細は下の解決済み記録。
- **第 90 弾**。 const 評価を probe して **div/modulo by zero・範囲外シフトの const がコンパイルエラーでなくランタイム panic になる**バグを検出・修正。 `const X: i64 = 10 / 0` は(`const_div_zero_error.il` fixture が compile error を期待するのに)モジュール init での panic だった。 const folder([consts.rs](../crates/ilang-parser/src/loader/consts.rs))の `fold_const_expr` が **単一の `Result<Expr, String>`** を返し、 caller が **全エラーを「非定数 → runtime init へ降格」扱い**(reason 無視)していたため、 div-by-zero(畳み込めるが不正)も runtime へ流れていた。 エラーを 2 種 `FoldErr::{NotConst, Invalid}` に分け、 div/mod by zero・範囲外シフトを `Invalid`(hard compile error)、 他を `NotConst`(従来どおり runtime fall-through)に。 top-level const と static field 両 caller が `Invalid` を `LoadError::BadConst` に。 既存 fixture は panic message の substring 一致で通っていただけ。 valid const 畳み込み・非定数の runtime 降格は無変更。 fixture: `05_edge_cases/const_modulo_zero_error.il`(compile 特有メッセージ「in const expression」を検査)。 詳細は下の解決済み記録。
- **第 89 弾**。 static フィールドを probe して **動的配列の static フィールドが checker を通るのに codegen で crash する**不整合を是正。 `static arr: i64[] = [1,2,3]` は checker が許可(メッセージも「dynamic arrays of numeric primitives」と明記)するのに、 読み出し/再代入で「mir-codegen: unsupported in M1: static slot type」、 init-then-use では **SIGSEGV**。 `LoadStatic`/`StoreStatic` は単語値(数値/bool/string ポインタ)しか扱わない。 codegen で Array を Str 同様 raw として扱う完全対応を試みたが SIGSEGV(init/ARC がより複雑で単純な Load/Store 追加では足りない)、 保守的に **checker 側で動的配列 static を拒否**([sigs.rs](../crates/ilang-types/src/checker/sigs.rs))— `array_of_prim_ok` を外し、 混乱する codegen crash を clean な診断に。 数値/bool/string static は無変更。 fixture: `05_edge_cases/static_array_field_error.il`。 詳細は下の解決済み記録。
- **第 88 弾**。 SIMD 構築を probe して **float リテラルが整数 SIMD レーンに無検査で通る**バグを検出・修正。 スカラ `let x: i32 = 1.0` は拒否されるのに `simd.i32x4 = [1.0, 2.0, 3.0, 4.0]` がコンパイルできた(レーンアクセス未公開なので値は観測不能だが、 公開時に garbage になる soundness 漏れ)。 原因は SIMD 構築検証([mod.rs](../crates/ilang-types/src/checker/mod.rs))が要素の「自身の型」に **lane 型を渡していた**(`dummy_vt = lane_ty`)ため、 全要素が自明に fit していた。 配列ケースと同じく `vt`(値の実型 `Array { elem }`)の要素型を `literal_assignable_with` に渡すよう修正。 副次的に over-wide 整数リテラル(`300` を i8 レーンへ)も正しく拒否されるように。 valid な構築(`i32x4` from int、 `f32x4` from float/int)は無変更。 fixture: `04_modules/simd_int_lane_float_literal_error.il`。 詳細は下の解決済み記録。
- **第 87 弾** (クリーンラウンド)。 @extern(C) 完了確認(全 `ExternCItem` 種別が pass 2 で検証されることを確認)と複数領域を sweep — **新規バグなし**。 メソッド/関数オーバーロード解決(型/サブクラス/兄弟曖昧拒否/引数数/幅/covariant-join arg/no-match 診断)、 `?` 演算子(Result ok/err 伝播・Optional・heap payload)、 Promise.all/race(型制約は仕様どおり)、 が全て健全。 唯一未カバーだった **`?` で heap payload の Result を伝播し err early-return が生きた Box 束縛を跨ぐ ARC**(`try_operator.il` は i32 のみ)を churn 厳密 deinit で pin。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT)。 fixture: `05_edge_cases/try_op_heap_result_arc.il`。
- **第 86 弾**。 第 84/85 弾の続きで **`@extern(C) struct` の field-type 検証**(第 84 弾は @bits のみ追加していた)の漏れを是正。 inline レイアウト不可の型 — 非 repr-c heap object・非最後の動的配列・tuple・optional・plain(非 repr)enum — が struct フィールドに通り、 heap object は **leak**(churn delta=2400, deinit=0)、 動的配列使用時は **panic**。 class 経路の field-type `ok` ロジックを `repr_c_field_ok` + `check_repr_c_struct_field` メソッド([decls.rs](../crates/ilang-types/src/checker/decls.rs))に抽出し、 `check_class` と extern_c の Struct arm([extern_c.rs](../crates/ilang-types/src/checker/extern_c.rs))で共有。 ただし **repr enum を許可型に追加**(struct は repr enum フィールドを許可 = `repr_c_flags_enum_field.il`、 class の旧 `ok` は未対応だった)。 非 repr-c object/tuple/optional/plain enum/非最後動的配列を明確な診断で拒否、 repr enum・nested repr-c struct・FAM(最後の動的配列)は維持。 fixture: `04_modules/extern_struct_heap_field_error.il`。 詳細は下の解決済み記録。
- **第 85 弾**。 第 84 弾(@extern(C) struct の @bits 検証漏れ)の同族として **`@extern(C) { union ... }` のフィールド検証が完全に抜けている**バグを検出・修正。 union は offset 0 を全フィールドで共有するため heap フィールド(object/string)は不正で `check_class` が拒否するが、 `ExternCItem::Union` 経路は Struct 同様 body 検証をスキップ。 結果 `union U { a: u64  b: Box }` がコンパイルでき、 `u.b = new Box(); u.a = 999; u.b.n` で stale ポインタを deref して **SIGSEGV**(実害あり)。 union 検証を free 関数 `validate_union`([decls.rs](../crates/ilang-types/src/checker/decls.rs))に抽出し、 `check_class` と `check_extern_c_bodies` の Union arm([extern_c.rs](../crates/ilang-types/src/checker/extern_c.rs))から共有。 heap union フィールド・空 union が明確な診断で拒否、 valid な数値 union(type punning)は無変更。 fixture: `04_modules/extern_union_heap_field_error.il`。 詳細は下の解決済み記録。
- **第 84 弾**。 `@bits` bitfield を probe して **`@extern(C) { struct ... }` の bitfield 検証が完全に抜けている**バグを検出・修正。 docs は `@bits(N)` を「unsigned 整数のみ・幅 ≤ 型幅」と明記し、 repr-C **class** 経路(`check_class`)はこれを enforce するが、 **`ExternCItem::Struct`** 経路は signature 登録のみで body 検証をスキップしていた。 結果 `@bits(4) x: i32`(符号付き)が通り符号拡張されず誤読(`-1` → `15`)、 `@bits(40) x: u32`(幅超過)も通る。 パーサは `1..=64` のグローバル境界のみ検査。 class 経路のインライン検証を free 関数 `validate_bitfield`([decls.rs](../crates/ilang-types/src/checker/decls.rs))に抽出し、 `check_extern_c_bodies`([extern_c.rs](../crates/ilang-types/src/checker/extern_c.rs))の Struct arm からも呼ぶよう配線(full `check_class` を呼ぶと struct 固有の enum フィールド等を誤拒否するので **@bits 検証のみ共有**)。 fixture: `04_modules/extern_struct_bitfield_signed_error.il` / `extern_struct_bitfield_overwidth_error.il`。 詳細は下の解決済み記録。
- **第 83 弾** (クリーンラウンド)。 新 subsystem を広く sweep — **新規バグなし**。 文字列メソッド連鎖 + コンテナ格納(`s.slice().toUpper().replace()` を配列/map へ)・数値変換端ケース(float↔int truncation・負の剰余/除算・NaN/Inf・f64 精度損失)・struct payload generic enum・loop break で heap 持ち出し・カリー化 closure が全て健全。 match の `if` guard と入れ子 enum パターンは docs 記載なし・clean な parse error で**意図的に未対応**(crash でない)、 `is` のフロー絞り込みも非対応(`as?` を使う = docs 通り)を確認。 唯一未 pin だった **struct-payload generic enum の covariant join**(`Wrap.one { item: Dog }` 形 — 第 79-81 弾 fixture は tuple-payload のみ)を既存 fixture に 1 ケース追加して pin。 fixture 追加のみのため重い儀式は省略(JIT harness + 個別 AOT で確認)。 fixture: `09_subtyping/generic_enum_covariant_join.il` に struct ケース追加。
- **第 82 弾** (クリーンラウンド)。 第 79-81 弾の join 共変が **interface 実装**(`Circle`/`Square` → `Shape`、 クラス階層を共有しないが共通 interface を満たす)と **Result の両 type param**(`Result.ok(Dog)`/`Result.ok(Cat)` → `Result<Animal, string>`、 Any 穴 merge と subclass join の合成)でも成り立つことを確認 — **新規バグなし**。 `as?` downcast(成功/失敗)・カリー化 closure・`is` 型テスト(narrowing は仕様どおり非対応、 `as?` を使う)・mixed-impl 配列リテラル推論も全て健全。 第 79-81 弾の fixture が class/array/map/tuple のみだった interface / Result の join を ARC 込み(churn delta=0・deinit 厳密)で pin。 fixture 追加のみのため AOT 全 suite・nested_generic 儀式は省略。 fixture: `09_subtyping/covariant_join_interface_result.il`。
- **第 81 弾**。 第 79/80 弾の join 共変ファミリを完成。 **map リテラル**(`{"k": Dog}`/`{"k": Cat}` → `Map<string, Animal>`)と **tuple**(`(Dog, 1)`/`(Cat, 2)` → `(Animal, i64)`)の if/match join が共変しない取りこぼしを是正。 単腕の map/tuple リテラル共変は docs 記載済みで、 第 80 弾でユーザーが承認した「fresh literal 共変を join に広げる」の残り(map/tuple とも mutable・literal-only で同じ健全性プロファイル)なので、 同原理の完成として実装。 `common_generic_join`([utils.rs](../crates/ilang-types/src/checker/utils.rs))に `Type::Tuple`(同 arity・各要素を共変 join)を追加(Map は `Type::Generic{base:"Map"}` なので既存 Generic 経路で対応済み)、 `is_covariant_join_literal` に `MapLit` / `Tuple` を追加、 `covariant_widening` に Tuple を追加。 literal-only ゲートは維持(別名 map/tuple の join は不変)。 map/tuple join の ARC churn は delta=0・deinit 厳密。 fixture に map/tuple ケース追加、 docs 更新。 詳細は下の解決済み記録。
- **第 80 弾** (ユーザー決定 = join 共変を配列リテラル / some 包みにも広げる)。 第 79 弾の継ぎ目を probe して、 join 共変が **配列リテラル**(`[Dog]`/`[Cat]` → `Animal[]`)と **`some(..)` で包んだ generic enum**(`some(Box<Dog>)`/`some(Box<Cat>)` → `Box<Animal>?`)に及んでいない取りこぼしを発見。 単腕の配列リテラル共変は docs 記載済み・素オブジェクトの some 合流も動くのに join だけ非対称だった。 配列は mutable で健全性が enum より微妙なため仕様判断としてユーザーに確認し、 **両方広げる**決定。 `common_generic_join`([utils.rs](../crates/ilang-types/src/checker/utils.rs))を Generic に加え **Array / Optional** を join するよう一般化(各 arg は `join_type_arg` で再帰)、 `is_covariant_join_literal` に Array リテラルと `some(eligible)` を追加。 literal-only ゲートは維持(別名配列の join は不変)。 ARC churn は配列 join で delta=0・deinit 厳密。 fixture `generic_enum_covariant_join.il` に array / opt ケース追加。 docs に追記。 詳細は下の解決済み記録。
- **第 79 弾** (ユーザー決定 = generic enum covariance を if/match join に広げる)。 型推論を probe して **generic enum リテラルの covariance が if/match の join を通らない**取りこぼしを是正。 `fn f(): Box<Animal> { Box.hold(new Dog()) }`(単腕)は通るのに、 `if b { Box.hold(new Dog()) } else { Box.hold(new Cat()) }: Box<Animal>` は「expected Box<Dog>, got Box<Cat>」で拒否されていた(plain object の if 合流は共通祖先を取るのに generic enum だけ非対称)。 join が両腕の型を共通祖先版 `Box<Animal>` へ合流する `common_generic_join` を追加し、 `check_if_expr`([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs))と match の `unify_branch_obj`([utils.rs](../crates/ilang-types/src/checker/utils.rs))に配線。 **literal 限定を厳守**(`is_covariant_join_literal` で全腕が ctor literal のときだけ発火、 別名 generic 変数は不変のまま — `generic_enum_literal_covariant_alias_error.il` の健全性を保持)。 両腕が同サブクラス(join=`Box<Dog>`)の場合は境界で `covariant_widening`(同じく literal ゲート)が `Box<Animal>` へ広げる。 開発中に alias 負テスト 2 件を一度壊して(型のみの判定で literal/alias を混同)literal ゲートで是正、 最終的に全緑。 docs(syntax.md / syntax_ja.md)に join 共変を追記。 fixture: `09_subtyping/generic_enum_covariant_join.il`。 詳細は下の解決済み記録。
- **第 78 弾** (クリーンラウンド)。 第 77 弾の `retain_if_heap` diverge を受けて、 lowering 内の手書き heap 型リストを全て canonical `MirTy::is_heap`(11 型)と照合 — **他に diverge は無し**(literals の 5 リスト・`fresh_arg_needs_post_release`・REPL slot の `value_to_i64`/`i64_to_slot_value` は全て完全)。 併せて Set の ARC を wrap 以外の配置で網羅 probe — 変数/Optional 再代入、 `?` の err 早期脱出が生きた Set 束縛を跨ぐ形、 Promise の Optional wrap(return)が全て値正・churn delta=0・deinit 厳密で **新規バグなし**。 第 77 弾が触っていない配置で Set ARC が一貫していることを pin。 fixture 追加のみのため AOT 全 suite・nested_generic 儀式は省略(JIT harness + 個別 AOT ビルドで確認)。 fixture: `05_edge_cases/set_arc_placements.il`。
- **第 77 弾**。 第 76 弾と同じ「wrap coerce の配置漏れ」を別の型で probe し、 **`Set` を `Optional` に bare wrap すると return 位置で set が早期解放される**バグを検出・修正。 `Set<Box>?` を `return s`(bare wrap)で返すと、 呼び出し側で `size()` が 0(set が空/dangling)。 `let os: Set<Box>? = s`(source が scope に残る)と明示 `some(s)` は読めていた(これが隠していた)。 原因は `retain_if_heap`([utils.rs](../crates/ilang-mir/src/lower/utils.rs))が canonical な `MirTy::is_heap` から **`Set` / `Weak` / `Promise` を取りこぼした手書きコピー**だったこと(コードベース自身が「per-site コピーは diverge する」と別所で注記済み)。 Optional-wrap coerce がこれを使うため set が retain されず、 source local の scope-exit release が set を解放、 返った Optional が空を指していた。 `retain_if_heap` を `ty.is_heap()` 委譲に変更(`Inst::Retain` は型ごとに `__retain_set`/`__retain_weak`/`__retain_promise` へ dispatch 済み)。 closure capture / field / arg / let / 明示 some の全配置で値正・churn delta=0・deinit 厳密を確認、 over-retain なし。 fixture: `05_edge_cases/optional_wrap_set_retain.il`。 詳細は下の解決済み記録。
- **第 76 弾**。 weak 参照 × コンテナ配置を網羅 probe する過程で **strong → `Node.weak?`(optional weak)の bare coercion が複数配置で壊れている**バグ群を検出・修正。 (1) `let w: Node.weak? = strongRef` と `Node.weak?` 引数は「no coercion from obj to weak?」で MIR lowering エラー、 (2) `obj.f = strongRef`(`Node.weak?` field)は素の strong ポインタを weak Optional スロットに格納し upgrade/release で **SIGSEGV**、 (3) borrowed strong を `Node.weak` / `Node.weak?` で **return すると leak**(borrowed-tail の strong retain を weak へ cast して孤児化、 deinit 不発 — これは既存バグで plain `Node.weak` 返却でも発生)。 明示 `some(strongRef)` だけが動いていた。 修正: `coerce` ([coerce.rs](../crates/ilang-mir/src/lower/coerce.rs)) が `Object → Optional<Weak>` を `lower_some_with_hint` と同じく分解(inner を Weak に downgrade → weak-rc share → box)、 `store_value_to_field` ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)) が同形を coerce 経路へ、 `emit_callee_retain` ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs)) が return 型が weak/optional-weak のとき Object tail の strong borrow-retain を抑制(bare `let w: Node.weak = s` が retain 無しで均衡するのと同じ)。 全配置(let/arg/field/return/tuple/array/map値/enum payload)で生存読み・死後 none・churn delta=0・deinit 厳密を確認。 docs は `Node.weak?`・downgrade・T→T? wrap を既に記載しその合成が直っただけなので変更なし。 fixture: `05_edge_cases/optional_weak_from_strong.il`。 詳細は下の解決済み記録。
- **第 75 弾**。 「ポインタ同一性 vs 値比較」の同族をクラスへ広げて probe し、 **`{}` map リテラル経由で作った `Map<MyClass, V>` が `@derive(Eq, Hash)` の値等価でなくボックスのポインタでキーする**バグを検出・修正。 `new Map<MyClass, V>()` は `$map.newObject(equals, hashCode)` を配線するのに、 map リテラル (`let m: Map<Point, V> = {}`) は `Inst::NewMap` に lowering され、 codegen が常にプリミティブ `__map_new` を呼んでいた。 結果、 値等価な別インスタンスで `has`/`get` が miss し、 同論理キーの再代入も上書きされず 2 エントリ残る (`Set` はリテラルが無く常に `new Set<>()` 経由なので無事 = 非対称が手掛かり)。 `new Map<>()` の object 構築を `build_object_keyed_map` ヘルパー ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)) に抽出し、 `{}` リテラル経路 ([literals.rs](../crates/ilang-mir/src/lower/literals.rs) の `lower_map_literal_with_hint`) でも key が value-equality class のとき同ヘルパーを使うよう修正。 map リテラルはクラスインスタンスのキーを綴れない (キーはリテラルトークンのみ) ので実害は空 `{}` 形のみだが、 それが慣用の空 map 記法。 docs は元から「Set / Map 両方に効く」と正しく記載済みのため変更なし。 fixture: `03_collections/map_object_key_brace_literal.il`。 詳細は下の解決済み記録。
- **第 74 弾**。 第 73 弾 (combined flags の `==`) の同族を probe して **repr enum 同士の順序比較 (`<` `<=` `>` `>=`) が tag でなくボックスのポインタを比較する**バグを検出・修正。 `Msg.paint < Msg.quit` (15 < 16) が **false**、 `Msg.quit < Msg.paint` (16 < 15) が **true** という具合に、 アロケータ/intern 順次第の誤答。 checker は repr enum 同士の比較を許可 (enum vs int リテラルは d41ca1b5 で全演算子対応済み) するのに、 lowering が enum-vs-enum を昇格せず `cmp_op` を生のオペランド (= ヒープアドレス) に適用していた。 `==` は第 73 弾の intern でポインタが canonical 化され偶然正しく読めていたが、 **順序はポインタ順が無意味なので intern では直らない**。 lower_binary ([ops.rs](../crates/ilang-mir/src/lower/ops.rs)) に「同型 enum の比較 (`==` `!=` `<` `<=` `>` `>=`) は両辺の tag を抽出し repr 整数として比較 (符号は repr 型が決める)」分岐を追加。 `==` / `!=` も intern 依存をやめて tag 抽出に統一。 非 repr enum の順序は従来どおり型エラー。 syntax.md / syntax_ja.md の repr enum 節に比較・順序の契約を 1 行追記。 fixture: `06_enums/repr_enum_comparison.il` (u32 / i32 repr で符号も pin)。 詳細は下の解決済み記録。
- **第 73 弾**。 直近の `@flags` enum (第 68/69 弾) の周辺を probe して **combined flags 値 (`read | write` など) の `==` / Set / Map / leak が同根で壊れている**バグを検出・修正。 `$enum.box` (bitwise / `~` / `int as Enum` cast の re-box) が毎回 rc=-1 の新規セルを確保していたため、 名前付き変異 (intern 済み singleton) と違い同ビットでもポインタが一致せず、 (1) `(read|write) == (read|write)` が false (等価は tag 上の定義のはず)・`!=` 反転、 (2) `Set` / `Map` が同一 combined 値を別要素として keying し dedup 失敗、 (3) 各 box が解放されず per-value leak、 の 3 症状を踏んだ。 `$enum.box` を codegen 側で intercept し、 結果の enum 型から local id を読んで `enum_global` でグローバル再マップ、 名前付き変異と同じ `__enum_unit_get((global_eid, disc))` intern キャッシュへ合流させた ([calls.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/calls.rs))。 これで combined 値が名前付き変異と同一の canonical ポインタになり 3 症状が一括解消、 alloc も値ごと 1 セルに収束。 dead化した runtime `__enum_box` / 宣言 / dispatch / jit 登録を撤去。 fixture: `06_enums/flags_enum_combined_identity.il`。 docs は元から「equality は discriminant tag 上」(syntax.md) と明記しており実装が追従しただけなので doc 変更なし。 詳細は下の解決済み記録。
- **第 72 弾**。 第 71 弾に続き panic 経路を probe して **無効な enum 値 (int から cast) を wildcard 無しの match にかけると SIGILL** になるバグを検出・修正。 `pub enum Msg: u32` に `99 as Msg` で宣言外の値を作り、 `match m { quit { } close { } }` (全 variant 網羅で checker は exhaustive と判断・`_` 無し) にかけると、 合成 default の `Terminator::Unreachable` が illegal-instruction trap (exit 132) を踏む。 match lowering ([match_.rs](../crates/ilang-mir/src/lower/match_.rs)) の no-wildcard default で Unreachable の前に `$ilang.panic("panic: no matching enum variant")` を emit (`Inst::Const` の `MirConst::Str` + builtin Call)。 codegen の builtin dispatch ([calls.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/calls.rs)) の `$xxx.yyy` 直接解決リストに `$ilang.panic` を追加 (既に declare 済み)。 非 repr enum では default は到達不能なので dead code。 `_` を使う安全形は無影響。 詳細は下の解決済み記録。
- **第 71 弾**。 panic / ランタイムエラー経路を probe して **`m[missing]` (Map の欠落キー index 読み) が docs と裏腹に panic せず default 値を返す**バグを検出・修正。 配列 OOB (`panic: index out of bounds`) ・unwrap none ・ゼロ除算は明確に panic するのに、 `m["missing"]` は i64 で 0 ・string で空文字 ・**object で null ポインタ**を返し、 後者は後続の使用で誤動作 (出力すら出ない)。 syntax.md は元から「missing key panics at runtime」と明記、 `m.get(k)` が安全版 (Optional)。 runtime `__map_get` ([maps.rs](../crates/ilang-runtime/src/maps.rs)) の `unwrap_or(0)` を、 欠落時に `rt_panic` ([print.rs](../crates/ilang-runtime/src/print.rs) 新設の Rust 呼び出し可能 panic ヘルパー) を呼ぶよう変更 — 配列 OOB と同じ exit 1。 詳細は下の解決済み記録。
- **第 70 弾** (クリーンラウンド)。 第 68/69 弾で直した repr/flags enum の周辺を網羅 probe — **新規バグなし**。 repr enum を Map キー / Set 要素 (int repr backing)・repr enum 算術 (`Color.red + 1`)・plain enum (repr なし) を Map キー・変異名 match・**u32→enum キャスト経由の match** (`match raw as Msg { quit ... }` = Win32 ディスパッチ形) が全て健全。 拒否される形 (enum を int リテラルで match / 生 u32 を変異 pattern で match) は明確な診断 + 動く代替 (変異名 / `==` / `as` キャスト) がある意図的制約。 未 fixture だった実用形を pin。 fixture 追加のみのため第 24/53 弾と同じく workspace / nested_generic 儀式は省略、 JIT・AOT 両経路で確認。 fixture: `06_enums/repr_enum_patterns.il`。
- **第 69 弾**。 `@repr` enum のビット演算周辺を probe して **repr enum を int リテラルと比較できない非対称**を検出・修正。 `msg == WindowMessage.destroy` (msg: u32) は enum-repr promotion で通るのに、 `Msg.close == 18` (enum == int リテラル) は「cannot apply binary op between Msg and i64」で拒否されていた (リテラルは i64 既定で、 promotion が「相手が repr に厳密一致」のときだけ発火するため u32 と一致せず)。 enum-repr promotion ([expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs)) に「repr enum vs repr に収まる int リテラル → enum を repr へ promote + リテラル採用」の 2 arm を追加。 両方向・全比較演算で動作、 変数比較は無影響。 詳細は下の解決済み記録。
- **第 68 弾**。 `@flags` enum のビット演算を probe して **2 件の lowering バグ**を検出・修正。 (1) **`f.has(other)`** — checker は合成 bool メソッド (`(f & other) == other`) として受理するのに **MIR lowering が無く**「method call on this type / unhandled builtin」。 `try_lower_flags_method` ([calls/mod.rs](../crates/ilang-mir/src/lower/calls/mod.rs)) を新設、 両 tag を `EnumTag` で抽出し AND して queried flag の tag と比較。 (2) **`~f` (BitNot)** — flags enum の **boxed 値 (ポインタ) に直接 `UnOp::Not`** し結果を enum 型にしていたため、 後続の tag 読み (`~f as u32`) が garbage を deref して **SIGSEGV**。 `~f as i64` 等で確定クラッシュ。 二項 `|`/`&`/`^` と同様に tag 抽出 → NOT → `$enum.box` で再 box ([ops.rs](../crates/ilang-mir/src/lower/ops.rs))。 詳細は下の解決済み記録。
- **第 67 弾**。 第 66 弾の covariance が **`some(..)` / tuple 等の入れ子降下では効かない**取りこぼしを是正。 第 66 は covariance を `value_assignable` (メソッド) に入れたが、 `some(Result.ok(new Dog()))` を `Result<Animal,string>?` へ入れる際の降下は `literal_assignable_with` (self を持たない自由関数) で起きるため未到達だった (配列/Map は hint-checker が要素ごとに value_assignable を呼ぶので元から動いていた)。 vt が既に推論済み型引数を持つことを利用し、 **型レベルの covariance 検査** (`type_covariant_to`: equal / Any / Object subtype / 構造的再帰) を `literal_assignable_with` の EnumCtor arm に追加。 これで some/tuple/array/深い入れ子 (`Result<Animal,string>?[]`) の全降下で合成。 第 66 のメソッド版 `enum_ctor_literal_covariant` は冗長になり削除。 詳細は下の解決済み記録。
- **第 66 弾** (ユーザー決定 = generic enum リテラルを covariant に)。 **generic enum のコンストラクタリテラルを型引数に対して covariant** にした (配列/Map リテラル covariance と一貫)。 `Result.ok(new Dog())` (`Result<Dog,_>`) を `Result<Animal, string>` へ、 `Result.err(new Dog())` を `Result<i64, Animal>` へ入れられる (ok=T 位置・err=E 位置の両方)。 ctor は宣言 enum (`Result<Animal,string>`) として記録され、 monomorphizer が親型を構築・payload は upcast 格納・親型越しの仮想ディスパッチが動く。 **リテラル限定** (別名の `Result<Dog,string>` 変数は `Result<Animal,string>` へ代入不可 — enum は immutable 値なので健全)。 value_assignable に `enum_ctor_literal_covariant`、 refine に subtype upcast (`is_covariant_upcast`) を追加。 詳細は下の解決済み記録。
- **第 65 弾**。 **async fn が generic enum を返すと型引数が refine されない**実バグを検出・修正 (第 63 弾の診断が表面化)。 `async fn getR(): Result<Box, string> { Result.ok(new Box(5)) }` は注釈なしだと「cannot infer the type parameter(s) of \`Result\`」。 async desugar は return 値を `Promise.$promise.settleResolve(p, Result.ok(..))` (generic static method `settleResolve<T>(p: Promise<T>, v: T)`) の引数に包むが、 **`resolve_method_call` ([method.rs](../crates/ilang-types/src/checker/method.rs)) の generic メソッド経路が引数の enum ctor を refine していなかった** (generic fn 経路・check_args は refine するのに)。 そのため `Result.ok(..)` の E=Any が残った。 検証ループに `refine_enum_ctor_args(arg, &actual)` を追加 — promise の要素型 `Result<Box,string>` が `v: T` に流れて ctor を refine。 async に限らず **全 generic メソッド呼び出し**の enum-ctor 引数 refine 漏れを修正。 heap payload は await 跨ぎで ARC 厳密。 詳細は下の解決済み記録。
- **第 64 弾**。 第 63 弾の明示診断が **enum-in-enum ctor の refine 漏れ**という実バグを表面化させ、 修正。 `Result.ok(Maybe.nope)` を `Result<Maybe<Box>, string>` に対して構築すると、 内側 `Maybe.nope` の T が refine されず (`refine_enum_ctor_args` が ctor の**自分の型引数**は埋めるのに **payload 引数へ再帰していなかった**)、 第 63 以前は Type::Any クラッシュ・第 63 以降は「cannot infer」診断が出ていた (が型は宣言から確定可能)。 `refine_enum_ctor_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) で (1) ネスト Any を含む slot (`Maybe<Any>`) を target の具体 arg で置換、 (2) **payload 引数を置換後の payload 型へ再帰 refine** するよう拡張。 let 注釈・map 値・fn 戻り・深い入れ子 (`Result<Maybe<Maybe<Box>>>`)・heap ARC で動作。 詳細は下の解決済み記録。
- **第 63 弾** (ユーザー決定 = 第 62 弾の残る限界を明示診断に)。 **型引数を解決できない generic 呼び出し / enum ctor を、 lowering クラッシュでなく checker の明示診断にした**。 これまで `match f(Result.err("e")) { .. }` (bare scrutinee) や `let r = Result.ok(5)` (E 未決定) は型引数が `Any` のまま lowering に届き「Type::Any (variadic builtins)」で停止していた。 `check()` ([check.rs](../crates/ilang-types/src/checker/check.rs)) の末尾に `report_unresolved_type_args` を追加し、 `enum_ctor_type_args` / `fn_call_type_args` の stash で **型引数に `Type::Any` が残るエントリ**を span 付きで報告 (「cannot infer the type parameter(s) of ... — add a type annotation」)。 `TypeVar` (generic テンプレート本体) は除外。 **挙動変更**: 従来コンパイルが通っていた「未呼び出し fn 内の曖昧 ctor」も明示エラーになる (`let r = Result.ok(5)` は到達性に関わらず曖昧 = Rust の `let r = Ok(5)` と同じく注釈必須)。 既存 fixture は曖昧 ctor に依存しておらず全緑。 詳細は下の解決済み記録。
- **第 62 弾** (第 60 弾の確認済み記録 (2) を大部分解消)。 **型を決める引数を持たない generic fn 呼び出し** (`f(Result.err("e"))`、 `fn f<T>(r: Result<T,string>)`) を、 **期待型が分かる位置 (let 注釈 / fn 戻り / 引数) で解ける**ようにした。 第 59 弾の `refine_fn_call_type_args` は fn の T を期待型から解いていたが、 **インライン引数 `Result.err("e")` 自身の stash が `[Any, string]` のまま**残り (まだ generic な param に対して check されたため) 引数が Type::Any で lower 失敗していた。 `refine_fn_call_type_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) に、 T を解いた後**各引数を具体化した param 型で再 `refine_enum_ctor_args`** する処理を追加 (引数は fn の T を共有するため)。 let 注釈・return・引数位置で動作、 heap T ARC 健全。 詳細は下の解決済み記録。 **残る限界**: 期待型が皆無の bare match scrutinee (`match f(Result.err("e")) { .. }`) は依然 Type::Any (本質的に曖昧・注釈が必要)。
- **第 61 弾** (第 60 弾の確認済み記録 (1) を解消)。 **match の arm が enum を yield し注釈なし let に束縛すると arm ctor が refine されず Type::Any** になる既存バグを修正 (非 generic でも再現)。 `let res = match r { ok(v) { Result.ok(v) } err(e) { Result.err(e) } }` は join で `Result<i64,string>` に解決するが、 各 arm の `Result.ok(v)` (E=Any) / `Result.err(e)` (T=Any) は片方しか pin せず、 `res` に注釈が無いため refine されなかった。 `check_match_expr` ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) / `check_match_optional` / `check_match_primitive` ([match_.rs](../crates/ilang-types/src/checker/match_.rs)) で join 後の結果型を各 arm に push する `refine_match_arm_ctors` を追加。 enum / Optional / primitive scrutinee の全 3 種に適用。 heap T ARC も健全。 詳細は下の解決済み記録。 **確認済み記録の (2)** (T を決める引数なしの generic 呼び出し) は未対応のまま。
- **第 60 弾**。 `?` を generic fn 内で使う形 (`fn unwrapOr<T>(r: Result<T,string>, fallback: T) { let v = r?; Result.ok(v) }`) を probe して、 **generic fn の型引数推論が引数の最初の binding を優先し Any を残す**既存バグを検出・修正。 `unwrapOr(Result.err("boom"), 0)` は arg1 `Result.err` が T=Any を入れ、 arg2 `0: T` の i64 で上書きされず、 fn が `<Any>` で具体化され monomorphizer が Type::Any で停止 (`?` 非依存。 `pick<T>(a: Result<T,string>, b: T)` でも再現)。 原因: `collect_type_var_bindings` ([sigs.rs](../crates/ilang-types/src/checker/sigs.rs)) が `or_insert_with` で最初の binding を優先。 **具体型が既存の Any を上書きする** (具体同士は従来どおり最初優先) ように修正。 詳細は下の解決済み記録。 **2 件の別系統の既存バグを記録**: (1) `let res = match someResult { ok(v){Result.ok(v)} err(e){Result.err(e)} }; res` (match arm が enum を yield・let に注釈なし) は arm ctor の型引数が refine されず Type::Any (非 generic でも再現。 match の join 結果型を各 arm に refine する必要)。 (2) generic fn を T を決める引数なしで呼ぶ (`f(Result.err("e"))` only) と `<Any>` 具体化で失敗 (match 文脈からの双方向推論が要る)。
- **第 59 弾** (第 57 弾の判断待ち記録 (2) を解消)。 **generic fn の型パラメータを戻り値位置から推論できる**ようにした。 `fn makeArr<T>(): T[]` / `fn wrapErr<T>(): Result<T,string>` など T が戻り型にしか現れない fn は引数から T を決められず Any のままだった (`let xs: i64[] = makeArr()` が「expected i64[], got any[]」、 enum 系は monomorphizer で Type::Any)。 `refine_fn_call_type_args(call, target)` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) を新設し、 fn の宣言戻り型を期待型に対して unify して残りの型パラメータを解き、 stash (`fn_call_type_args`) を更新して補正後の戻り型を返す。 期待型が分かる 3 位置 — let 注釈 ([stmt.rs](../crates/ilang-types/src/checker/stmt.rs))・fn 戻り位置 ([decls.rs](../crates/ilang-types/src/checker/decls.rs))・呼び出し引数 ([method.rs](../crates/ilang-types/src/checker/method.rs)) — で適用。 部分推論 (片方を引数・片方を注釈) ・heap T の ARC も健全。 詳細は下の解決済み記録。 **これで第 57 弾の判断待ち記録 2 件は両方解消。**
- **第 58 弾** (第 57 弾の判断待ち記録 (1) を解消)。 **generic class メソッドが generic enum を構築すると「unknown enum 〜」で lower 失敗**する既存バグを修正。 class 単一化 ([class.rs](../crates/ilang-mir/src/monomorphize/class.rs)) の `subst_expr` は specialized method body の型を置換するが **enum ctor の `enum_name` を再 mangle しなかった**ため、 `class Wrap<T> { asSome(): Maybe<T> { Maybe.some(this.v) } }` を `Wrap<i64>` で使うと `Maybe.some` が bare のまま (builtin Result も同症状)。 fn 経路 (fns.rs) は再 mangle するが class 経路に無かった。 checker の `enum_ctor_type_args` を thread-local 経由で class pass に渡し、 `subst_expr` が span 記録の型引数を class の T→具体で置換して mangle (fn 経路と同形)。 builtin Result・user enum・入れ子 (`Result.ok(Maybe.some(this.v))`)・複数インスタンス化・match 消費・heap T payload ARC を網羅。 詳細は下の解決済み記録。 **判断待ち記録の (2)** (generic fn 戻り型のみからの型推論) は未対応のまま。
- **第 57 弾**。 generic fn × enum ctor の継ぎ目を突き、 **generic fn が builtin `Result` を構築/返却すると「unknown enum Result」で lower 失敗**する既存バグを検出・修正。 `Result<T,E>` は宣言が無く call site ごとに monomorphize されるが、 `monomorphize_fns` は specialize 時に generic-enum の `EnumCtor.enum_name` を mangle する判定材料 (`generic_enums`) を**プログラム宣言の enum だけ**から作っていたため builtin Result が漏れ、 `fn wrapOk<T>(x: T): Result<T, string> { Result.ok(x) }` の `Result.ok` が `enum_name="Result"` のまま残った (user 宣言の `Maybe<T>` は含まれるので動く)。 `monomorphize_fns` の `generic_enums` に Result テンプレートを seed して修正 (monomorphize_enums パスと同じ対処)。 型パラメータが引数で決まる形 (ok arg / err arg / 両腕) で動作・heap payload ARC 厳密。 詳細は下の解決済み記録。 **2 件の別系統の既存バグを記録** (本弾では未対応): (1) generic **class メソッド**が generic enum (user の `Maybe<T>` も builtin `Result` も) を構築すると「unknown enum 〜」 — class 単一化経路が method body の enum ctor を mangle しない別バグ。 (2) generic fn の型パラメータを**戻り値注釈のみ**から推論できない (`let xs: i64[] = makeArr<T>()` も `Maybe`/`Result` も同様) — generic-fn 戻り型推論一般の制約。
- **第 56 弾**。 `?` 演算子 × enum ctor 型引数精緻化の継ぎ目を突き、 **`?` が呼び出し引数の中に入れ子になると Type::Any で lower 失敗**する既存バグを検出・修正。 `?` は `err(e) { return Result.err(e) }` (T=Any, E=string) を含むブロックに desugar されるが、 `Result.ok(take(g(ok)?))` のように `?` が tail 式の奥 (call 引数 / some / tuple / array 要素) に埋まると、 その `return Result.err` が refine されず T=Any のまま monomorphizer に届いていた。 原因: `refine_enum_ctor_args_in_block` が**文**の値は `refine_returns` で深く歩くのに、 **tail** は自身の値 ctor しか refine していなかった。 tail にも `refine_returns` を適用して修正。 `let b = g()?` (自前の文) は元から動いていた。 詳細は下の解決済み記録。
- **第 55 弾** (ユーザー決定 = 第 54 弾の別件に対し「`{}` でも書けるように実装」)。 空マップリテラル `{}` を **型注釈が `Map<K,V>` の位置で空マップとして解釈**するようにした (JS 風の利便性。 `new Map<K,V>()` は従来どおり)。 パーサは `{}` を空ブロックとして出すので型情報の要る checker + lowering で対応: checker `value_assignable` が空ブロック (stmt/tail なし) を Map ターゲットへ受理、 lowering `lower_composite_with_hint` が `(Block 空, MirTy::Map)` を空 `NewMap` に、 `is_fresh_object_expr` が空ブロックを fresh 扱い (scope-exit 解放のため。 unit 文脈では no-op)。 let / 再代入 / field 代入 / fn 戻り値 / fn 引数 / ネスト map 値で動作、 unit 文脈の空ブロックは不変。 詳細は下の解決済み記録。
- **第 54 弾**。 第 48〜50 弾の enum ctor 型引数精緻化の **残る store 位置 3 系統**を検出・修正。 (1) **enum payload 引数** (`Wrapper.wrap(Result.err(..))` tuple / `WrapperS.wrapS { r: Result.err(..) }` struct) — `check_enum_ctor` が自分の引数を payload 型へ refine していなかった。 (2) **built-in 配列メソッド引数** (`xs.push(Result.err(..))` / unshift / fill / remove / indexOf / includes) — ハードコード経路が element 型へ refine していなかった (Map.set / Set.add は check_args 経由で既に健全)。 (3) **index 代入** (`xs[0] = Result.err(..)` / `m["k"] = Result.err(..)`) — `check_assign_index` が element / value 型へ refine していなかった。 いずれも `Result<_, string>` 等の T=Any が monomorphizer に届き「Type::Any (variadic builtins)」で停止していた既存バグ。 詳細は下の解決済み記録。 **別件の既存ギャップを記録**: 空マップリテラル `{}` は型注釈に対して `()` (空ブロック) と推論され `let m: Map<K,V> = {}` が型エラー — パーサが `{}` を空ブロックとして出すため。 (※当初「空マップの構文が無い」と記したのは誤り。 `new Map<K,V>()` が既存の確立した形。 `{}` でも書けるようにする利便性として第 55 弾で対応。)
- **第 53 弾** (クリーンラウンド)。 第 52 弾の slot 昇格修正の周辺と Map forEach の反復中 mutation を網羅 probe — **新規バグなし**。 メソッド内/accessor 内クロージャ・init・static メソッド・default 引数式からのグローバル参照は全て昇格される。 forEach の add-during / future-key-delete (snapshot が +1 で生存させ UAF なし) / nested forEach は deinit 厳密 + delta=0。 forEach mutation の ARC を pin。 詳細は下の確認済み記録。
- **第 52 弾**。 property accessor を probe して、 **getter/setter 本体から top-level let (グローバル) を参照すると「unbound variable」で lower 失敗**する既存バグを検出・修正。 slot 昇格判定 (`build_slot_table` → `collect_fn_free_var_refs`) がクラスの method / static method は走査するのに **property accessor body を走査していなかった**ため、 getter/setter だけが参照する let が host slot に昇格されなかった (method は動くのに accessor は不可)。 `collect_fn_free_var_refs` の `walk_class` に getter/setter body 走査を追加。 詳細は下の解決済み記録。
- **第 51 弾**。 static フィールドを probe して、 **heap (string) static フィールド代入が値を retain せず use-after-free**する既存バグを検出・修正。 `Cls.s = arg` (借用 param に fresh string) が static slot を所有せず、 源の transient +1 解放後に slot が dangling → 後続 read が解放済みバッファ (fresh string が空印字)。 `StoreStatic` に instance field 同様の retain-new (非 fresh) / release-old を追加。 詳細は下の解決済み記録。
- **第 50 弾**。 第 48/49 弾の enum ctor 型引数精緻化の **最後の穴 = クロージャ本体の戻り値**を是正。 top-level fn の戻り値は refine していたが、 `fn(): Result<i64,string> { Result.err(..) }` や `array.map(fn -> Result<...>)` のクロージャ tail / 早期 return が未配線で Type::Any だった。 `check_fn_expr` に `refine_enum_ctor_args_in_block` を追加。 詳細は下の解決済み記録。
- **第 49 弾**。 第 48 弾の enum ctor 型引数精緻化の **残る store 位置**を是正。 引数位置 (fn / method / init)・`some(..)`・tuple 要素でも `Result.err(..)` 等が Type::Any で失敗していた。 `refine_enum_ctor_args` を some / tuple / array へ**再帰**させ (入れ子も伝播)、 call-arg checker (`check_args` / generic fn arg / fn 型 call) に refine を追加。 詳細は下の解決済み記録。
- **第 48 弾**。 Result `?` を probe する過程で、 **型パラメータを引数から推論できない enum コンストラクタ (`Result.err(..)` は T、 `Maybe.nope` は両方) を field / 配列リテラル / Map 値に格納すると Type::Any で lower 失敗**する既存バグを検出・修正。 `let f: T = ctor` は注釈で enum ctor の型引数を精緻化 (`refine_enum_ctor_args`) するのに、 field 代入 (明示 + bare)・配列リテラル要素・Map 値・local 再代入は精緻化していなかった。 結果 2 型パラメータ enum (Result / Either) を field/配列に置けなかった。 5 箇所に refine を追加。 詳細は下の解決済み記録。
- **第 47 弾** (クリーンラウンド)。 第 46 弾で開いた interface 周辺の **covariance と downcast** を網羅 probe — **新規バグなし**。 interface 実装クラスを Optional wrap / enum payload / Map 値 / 入れ子コンテナ / generic 型引数 / tuple 要素 (wrap 込み) に流し込む covariance、 `as?` による interface→具象 downcast (成功=共有・失敗=none) を deinit 厳密 + delta=0 で確認。 subclass の wrap/covariance/downcast 機構が interface にも generalize。 詳細は下の確認済み記録。
- **第 46 弾**。 interface dispatch を probe して、 **異なる interface 実装クラスの if/else・match・配列リテラルが共通 interface に合流できない**型推論バグを検出・修正 (ユーザー判断 = 「あるべき形に修正」)。 分岐 join が `common_ancestor` (クラス階層) しか見ず、 共通 interface を join 先にしていなかった (subclass は動くのに interface は不可)。 `common_object_join` を新設し全 object-join 箇所 (if / match / 配列 / Map リテラル) を切替。 唯一の共通 interface に合流、 複数共通 interface は曖昧として型エラー。 詳細は下の解決済み記録。
- **第 45 弾** (クリーンラウンド)。 weak 参照と固定長配列 × wrap を probe — **新規バグなし**。 weak.get() の昇格 (生存=値 / 死後=none)・weak? back-ref サイクル・parent-owns-child cascade (二重解放なし)・weak 配列、 および **固定長配列 `T[N]` の要素 wrap** (`Box?[2]` リテラル/index store、 `(Box?, Box)[2]` への tuple index store) を deinit 厳密 + delta=0 で網羅。 wrap 修正 (第 36/41) が固定長表現にも generalize していることを確認。 詳細は下の確認済み記録。
- **第 44 弾** (クリーンラウンド)。 第 43 弾の周辺を string/array ARC 全方位で probe — **新規バグなし**。 string メソッド連鎖の fresh 中間・template literal の heap 補間・`+=` desugar・**self-concat `s = s + s`** (aliased rhs を解放しない正しい挙動)・split・array push/unshift/map の fresh 要素・heap-kind 変数の fresh 再代入を、 `liveStringCount` / deinit 厳密で網羅。 string-ARC 形を pin。 詳細は下の確認済み記録。
- **第 43 弾**。 string バッファ ARC を probe して、 **inplace concat `s = s + n.toString()` が fresh な rhs 文字列を 1/iter リーク**する既存バグを検出・修正。 `StrConcatInplace` は rhs を `s` のバッファに**コピー**するだけで消費しないため、 fresh rhs (`toString()` / fresh concat) の +1 が宙に浮く。 リテラル rhs (intern 済み) や借用 var rhs は無事。 op 後に fresh rhs を Release。 詳細は下の解決済み記録。
- **第 42 弾** (クリーンラウンド)。 composite 要素 wrap の **残る store サイト**を網羅 probe — **新規バグなし**。 return 位置・`some(tuple)`・enum payload・引数位置・local 再代入・明示 field 代入・入れ子 `((Box?, Box), Box?)`・tuple 内 weak 要素を、 Optional 越し match で実体確認しつつ deinit 厳密 + delta=0。 第 36 (bare field) / 第 41 (index store) で未 pin の位置を 1 本に pin。 詳細は下の確認済み記録。
- **第 41 弾**。 tuple をコンテナに格納する ARC を probe して、 **index 代入 `coll[i] = (box, b)` が tuple 要素の wrap を欠く** 既存 SIGSEGV を検出・修正。 `arr[0] = (box, b)` (`(Box?, Box)[]`) / `m["k"] = (box, b)` (`Map<_, (Box?, Box)>`) が slot0 を wrap せず生 Box を `Box?` slot に格納 → 解放時クラッシュ (リテラル `[(box,b)]` は元から hint 済みで無事)。 `AssignIndex` の RHS を要素型ヒント付き lowering に変更 (第 36 弾と同型)。 詳細は下の解決済み記録。
- **第 40 弾**。 第 39 弾の tuple subst 修正の **同族探索が不完全**だったのを是正。 型パラメータ置換 (`subst_type`) だけでなく、 **concrete な generic instantiation を mangle する rewrite 群**も tuple を見落としていた: `rewrite_type` (generic class)・`rewrite_enum_refs_in_type` (generic enum)・`walk_types_pre` (instantiation 発見)。 `(Inner<i64>, i64)` / `(Maybe<i64>, i64)` を field / param / return に使うと「unsupported in M1: user-defined generic types」で停止。 3 関数に `Type::Tuple` arm を追加。 詳細は下の解決済み記録。
- **第 39 弾**。 generic + heap を probe して **monomorphize が tuple 型の中の型パラメータを置換しない**既存バグを検出・修正。 `(T, T)` / `(T?, T)` を generic fn/method のシグネチャや field 型に使うと「unknown type: T」で lowering 停止 (Optional / array / Map は置換されるのに tuple だけ漏れ)。 `subst_type` / `contains_type_var` に `Type::Tuple` arm を追加。 詳細は下の解決済み記録。
- **第 38 弾** (クリーンラウンド)。 第 37 弾の exit-drain 修正の周辺を async 全方位で probe — **新規バグなし**。 timer (setTimeout) at exit・Promise.all over heap・Promise を field 保持・multi-await chain・never-settled promise の executor heap capture・**await rejection が await 前の heap local を解放**・reject 経路の shutdown drain を網羅し、 全て deinit 厳密 + leak なし + clean exit。 await-rejection ARC と reject 経路 drain を pin。 詳細は下の確認済み記録。
- **第 37 弾**。 async へ攻撃面を移し、 **exit 時の event-loop drain がグローバル解放の後に走る順序バグ**を検出・修正。 pending promise 継続が保持する Box の `deinit` が top-level 配列 `deinits[0]` に触ると、 シャットダウンで解放済みグローバルを参照して **ランタイム OOB panic** (`deinit` 持ちの heap を await 跨ぎで保持する最小形で再現)。 drain を `__main` の top-level let 解放の **前**に emit して修正。 詳細は下の解決済み記録。
- **第 36 弾**。 bare field 代入の **composite リテラル要素 wrap 欠落** という既存 SIGSEGV を検出・修正。 `pair = (box, b)` (field 型 `(Box?, Box)`) が tuple を `(Box, Box)` として構築し生 Box を `Box?` slot に格納 → 解放時に不整列ポインタ参照でクラッシュ。 bare 経路がヒント無し lowering を再利用していたのが原因 (第 33 弾の設計の取りこぼし)。 field 型を composite ヒントに渡して修正 (array `Box?[]` / map `Map<_,Box?>` の同族も同時に解消)。 詳細は下の解決済み記録。
- **第 35 弾** (クリーンラウンド)。 bare field 書き (第 32 弾) × 連鎖レシーバ解放 (第 29/30 弾) の継ぎ目を反復ミューテーション・多段連鎖で攻めた — **新規バグなし**。 init 内 bare 再代入の `is_init` 安全性 (checker が初回 `this.` を要求して担保)、 fixed-array bare 代入の copyShallow、 4 段連鎖・field 返し連鎖、 **エスケープしたステートフルクロージャの heap field 反復付け替え** (deinit 700 厳密) を pin。 詳細は下の確認済み記録。
- **第 34 弾** (クリーンラウンド)。 第 33 弾で統合した `store_value_to_field` の継ぎ目を「bare field 代入 × 宣言型が要る RHS」で攻めた — **新規バグなし**。 共変 map / 配列リテラル・fresh Optional・Optional widen の bare 代入を厳密 deinit + churn delta=0 で pin。 併せて **第 33 弾の誤記録を訂正** (「weak の bare 代入は明示と非対称」は誤り。 実体は `none → plain Box.weak` の拒否で bare/明示共通・意図的制限)。 詳細は下の確認済み記録。
- **第 33 弾**。 第 32 弾で触った **implicit bare-name field 代入** arm が、明示 `this.f = v` (AssignField) と違って **`T → T?` / subtype の Optional 自動 wrap を欠く** 既存バグを検出・修正。 `slot = box` (field 型 `Box?`) が生オブジェクトを Optional slot に格納し解放時に **SIGSEGV**。 両経路を共通ヘルパー `store_value_to_field` に統合。 詳細は下の解決済み記録。
- **第 32 弾**。 第 31 弾のクロージャ `this` 捕獲の継ぎ目を攻めて **コンパイラ panic 2 件** を検出・対処。 (1) メソッド内クロージャが **bare field に代入** (`slot = nb`) すると lowering が panic (修正)。 (2) クロージャ内の **bare メソッド呼び出し** (`compute()`) が panic → クリーンな診断に (ユーザー決定)。 詳細は下の解決済み記録。
- **Promise/async 実行モデルを JS 型 (run-to-completion・シングルスレッド) へ移行**。 worker pool を撤去し、 継続はメインスレッドの FIFO queue + 期限順 timer heap (`pool.rs` 書き換え)、 executor は構築時に同期実行、 非ブロッキング pump の **`time.tick()`** を新設。 詳細は下の解決済み記録。
- **gui ライブラリの platform イベントループに `time.tick()` pump を組み込み** (cocoa = NSTimer common modes / win32 = SetTimer TIMERPROC / linux = g_timeout_add、 各 ~15ms)。 GUI 表示中も std.time タイマーと Promise 継続が発火する。 **win32 / linux は macOS 環境では型検査されないため未検証** — 詳細は下の解決済み記録。
- **fixture 増殖ラウンド第 8 弾** (新実行モデル周辺)。 `new Promise(executor)` の 3 セル leak (移行前から) と、 release ビルドの関数マージ (ICF) による capture 二重登録 → promise 二重解放 (leak 修正で顕在化した潜在バグ) を検出・修正。 回帰 fixture 4 件、 `ILANG_DEBUG_PROMISE/CLOSURE/TIMER` トレースを常設化。 詳細は下の解決済み記録。
- **fixture 増殖ラウンド第 9 弾**。 float 型 promise の executor 経路の ABI 不一致 (garbage / SIGSEGV、 既存)、 fresh 引数 post-release の残り 2 経路 (クロージャ間接呼び出し / 暗黙 `this.method`、 既存) + promise `.then`/`.catch` の fresh receiver release 欠落 (既存)、 armed timer を残した panic 終了の TLS 破棄順 abort (新モデル起因) を検出・修正。 回帰 fixture 4 件。 詳細は下の解決済み記録。
- **fixture 増殖ラウンド第 10 弾**。 `Promise.all`/`race` の入力配列未消費 (既存)、 all 結果配列の要素所有権欠落 (1 の修正で UAF 顕在化)、 fresh promise 引数の release 欠落 (5 サイトを共通述語 `fresh_arg_needs_post_release` に一本化) を検出・修正。 回帰 fixture 3 件。 REPL が `use` 文未対応という独立の既存制限も記録。 詳細は下の解決済み記録。
- **第 11 弾**。 (1) **await の rejection propagate を実装** (ユーザー決定 = JS 意味論: await 先が reject したら async fn の結果 promise も同じ msg で reject。 旧挙動は無言で永久 pending)。 (2) **早期 `return` が生きている heap 束縛を release しない一般バグを修正** (`lower_return` に sweep が無かった — fn 直下 / match arm / loop body / async poll fn 全部で 1 個/call leak)。 回帰 fixture 2 件。 詳細は下の解決済み記録。
- **第 12 弾**。 `continue` の sweep 欠落 (既存)、 fresh match scrutinee が diverging arm で leak (既存)、 string match の fresh scrutinee 全面未解放 (既存)、 async fn 本体の早期 `return` の実装 (旧: 型エラーで不支持)、 引数なし fn で第 11 弾 sweep が空振りする取りこぼし (deinit 不発で発見) を修正。 回帰 fixture 2 件。 詳細は下の解決済み記録。
- **第 13 弾**。 for-in × 早期 return の両方向: fresh iterable の leak (既存) と、 **第 11 弾 sweep が入れた要素借用の過剰解放 (UAF) の退行**を修正。 要素束縛は `PatternBinding` (借用) へ再分類、 `env.bind` 全箇所を監査済み。 回帰 fixture 1 件。 詳細は下の解決済み記録。
- **第 14 弾**。 deinit カウントを過剰解放検出器に使い、 早期脱出 × 値の持ち出し (payload の return/break・closure escape・入れ子 for-in 貫通・async 内制御構造) を網羅 — **新規バグなし**。 pin 用 fixture 1 件。 詳細は下の確認済み記録。
- **第 15 弾**。 REPL のパイプラインを loader 相当の normalize 経路 + fresh TypeChecker + slot 型の monomorphize 注入に乗せ替え — enum / async fn / const / generic 型 let の chunk 跨ぎを修復、 `use` は明示診断に。 `?` 演算子は実は Result 用に実装済みで健全 (HANDOFF の未実装リストが古かった)。 未知属性の素通り・turbofish 誤パース・`?` on Optional の誤診断を記録。 repl.rs +6 件。 詳細は下の解決済み記録。
- **第 16 弾**。 REPL の**型違い re-let が無言の型穴** (string ポインタ生値の印字) になっていたのを明示エラーで封鎖。 fn / class の再定義セマンティクス (現状: 拒否 / 生エラーで旧定義が残る) は設計判断待ちとして記録。 repl.rs +3 件。 詳細は下の解決済み記録。
- **第 17 弾**。 weak × 早期 return・property accessor 本体・fs/path/Unicode・interface 配列 × for-in × return・repr/@flags enum を網羅 probe — **新規バグなし** (クリーンラウンド 2 回目)。 pin 用 fixture 1 件。 詳細は下の確認済み記録。
- **第 18 弾**。 **固定長配列 `T[N]` × heap 要素の ARC 未モデル** (codegen が自認していたギャップ) が実害として確認された: scope exit / 早期脱出 / field drop / field 上書きで要素が漏れる (store の rc と escape は正常 — fixture で pin)。 詳細は下の解決済み記録 (第 19 弾で解決)。
- **配列リテラルの推論規則を変更 (ユーザー決定)**: 注釈なしの `let xs = [...]` は**動的配列** `T[]`、固定長 `T[N]` は宣言型 (注釈・field・引数) が要求した時だけ。旧規則 (無注釈リテラル = `i64[3]` 固定長、push には `i64[]` 注釈が必要) を廃止。`literal_assignable` が式の長さを直接見るため固定長注釈への代入は無変更で通る。これに伴い generic 型引数への配列リテラル直渡し (`hold([new Box(1), new Box(2)])`) は T=Box[] に束縛され自然に通るようになり、式レベル検査のリテラル除外ハックを削除。fixture: `01_basics/array_literal_dynamic_inference.il`。
- **第 31 弾**。 第 30 弾で記録した **capturing closure を class field に持つとリーク** するバグを修正。 原因は closure の自由変数収集 ([collect.rs](../crates/ilang-mir/src/lower/collect.rs)) が **bare `Var` ごとに `this` を投機的に候補追加**していたこと (「その名前が field なら `this.field` だから」)。 フィルタは「`this` が env で解決可能なら残す」だけなので、 メソッド本体内のクロージャでは `this` が常に解決可能 → **plain local / param しか参照しないクロージャまで `this` を捕獲**。 `this.f = fn(){ new Box(x) }` (x は param) が `this → closure → this` のサイクルを作り、 オブジェクトごとリークしていた。 修正: 投機的 `this` 追加を撤廃し、 [fn_expr.rs](../crates/ilang-mir/src/lower/fn_expr.rs) で **free var が enclosing class の実メンバ (field / property、 親チェーン込み) に解決された時だけ on-demand で `this` を捕獲** (`name_is_this_member`)。 明示 `this.` アクセスは従来どおり `frees` 経由で捕獲。 method は対象外 (bare メソッド呼び出し `compute()` は元から captured-this 経由で解決されず、 含めると新規 panic を生む — `this.compute()` を使う既存制限のまま)。 診断は MIR dump で `make_closure func#3 (v1, v0)` の v0=this が未使用 capture と判明、 HEAP_TRACE で「オブジェクト+closure が共に free されない」ことを確認。 bare field 参照クロージャ (genuine な `this` 捕獲) は維持 — それを field に格納すれば真の ARC サイクル (設計上のリーク、 weak で断つ) になる点も probe 済み。 fixture: `05_edge_cases/closure_field_no_spurious_this.il` (param/local 捕獲・build-only・bare-field escape を厳密 deinit 300 + churn delta=0)。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 31 弾の回帰修正**。 第 31 弾の on-demand `this` 捕獲が `lookup_var` だけで `this` を解決していたため、 **nested closure (内側 closure が field を bare 参照)** が panic していた (内側の `this` は外側 closure の capture 経由で local ではない)。 修正: needs_this を後段で個別 capture するのをやめ、 **`this` を `frees` に追加して通常の free-var 解決 (local も `captures_in_scope` forward も両対応) に通す** 方式へ ([fn_expr.rs](../crates/ilang-mir/src/lower/fn_expr.rs))。 direct method body (local this) と nested closure (forwarded this) の両方が動作。 fixture: `05_edge_cases/nested_closure_field_capture.il` (2 段ネスト + 明示 this を厳密 deinit 200 + churn delta=0)。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 30 弾**。 第 29 弾で直した連鎖メソッドの fresh-receiver リークの **同族 2 経路** (interface dispatch / closure-field call) に同じガードが残っていたのを検出・修正。 `lower_iface_dispatch` と `lower_fn_field_call` ([object.rs](../crates/ilang-mir/src/lower/calls/object.rs)) が共に `obj_is_fresh && !matches!(ret, Object)` で、 `mkHolder().grab().n` (interface 経由) と `mkObj().fnField().n` (closure-field 経由) の中間 receiver をリークしていた。 修正: Object 戻り値でも fresh receiver を解放。 interface VirtCall は ilang クラスにしか届かない (COM は別経路 `lower_com_iface_dispatch`、 @objc は objc_msgSend) ため @objc 除外不要、 closure-field call は戻り値が closure の所有戻り値で receiver 別名ではないため同じく不要。 static メソッド連鎖 (`Factory.make().grab()`) は第 29 弾の通常メソッド経路を通り元から健全 (回帰ガードとして fixture に保持)。 fixture: `05_edge_cases/chained_receiver_iface_fnfield.il`。 検証: nextest 539/539 (cocoa 含む)、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 **別系統の既存バグを記録** (背景タスク化): **capturing closure を class field に持つとオブジェクト解放時に closure block がリーク** (`class F { f: fn(): Box; init(x){ this.f = fn(): Box { new Box(x) } } }` をローカルに作るだけで 56 bytes/round 漏れる。 非 capturing closure field は健全)。 連鎖修正とは独立、 closure-capture 系の解放漏れ。
- **第 29 弾**。 **連鎖メソッド呼び出し `a.m1().m2()` で、m1 が Object を返す時に中間 receiver がリークする** 一般バグを検出・修正。 当初 generic クラス (`Cell<Cell<Box>>`) の leak として浮上したが、 切り分けで **generic 非依存**と判明 — 非 generic の `o.get().get().n` でも漏れる。 原因: object メソッド lowering の fresh-receiver 解放 ([object.rs](../crates/ilang-mir/src/lower/calls/object.rs)) が **戻り値が非 Object の時だけ** Release を出していた (`!matches!(sig.ret, Object)`)。 「フィールド別名を返す場合に過剰解放しないため」の保守的ガードだったが、 ilang メソッドの heap 戻り値は常に所有 (+1 — tail-borrow / bare-`this` retain) なので、 返り値は receiver の drop cascade を生き残る。 結果 `obj.getThing().use()` ・ builder `mk().bump().bump()` ・ 入れ子 generic の `cc.get().get()` がすべて中間値をリークしていた (`let m = a.m1(); m.m2()` の名前付き中間は scope-exit 解放で無事)。 修正: Object 戻り値でも fresh receiver を解放、 **ただし `@objc` receiver は除外** (ObjC の Object 戻り値は autorelease の借用で +1 所有ではない — `handle: i64` フィールドの有無で判定、 `lower_field` の receiver-temp 判定と同じ)。 診断は HEAP_TRACE + `__release_object` の rc トレースで「中間 Cell が rc=1 で残る」ことを確認して確定。 fixture: `05_edge_cases/chained_method_receiver_release.il` (field 連鎖・builder・入れ子 generic・名前付き中間ガードを厳密 deinit 500 + churn delta=0)。 検証: nextest 539/539 (cocoa_foundation/appkit 含む — ObjC 除外が効いている)、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 28 弾**。 第 26 弾の wrap 拡張が **狭すぎた** ことによる既存バグを probe で検出・修正。 第 26 弾は `T → T?` 自動 wrap を「plain `Object → Optional<Object>`」に限定したため、 **object 形だが Object でない subtype 源** — 特に `Dog[] → Animal[]?` (`Array<Dog> → Optional<Array<Animal>>`) — が抜け、 `let m: Map<string, Animal[]?> = {"a": [new Dog()]}` が lower 時「no coercion from obj[] to obj[]?」で停止 (checker はリテラル covariance で通すのに)。 store 位置なら第 26 弾と同じく crash 系統。 修正: wrap 述語を 3 箇所 (`coerce` [coerce.rs](../crates/ilang-mir/src/lower/coerce.rs)・`release_owned_wrap_source` [body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs)・`AssignField` の `needs_optional_wrap` [expr.rs](../crates/ilang-mir/src/lower/expr.rs)) で「inner も from も object 形 (Object/Array/Tuple/Map/Optional)・ただし from 自身は `Optional<_>` でない」へ拡張 (Optional 源は Optional→Optional widen のため除外し、 `none` / `Box??` を不変に保つ)。 同族確認: map-of-arrays・nested map・array-of-maps・`Animal[]?` wrap を let/field/arg/return で値・ARC 厳密一致 (deinit 900 + churn delta=0)、 none/混在/`Box??` の回帰なし、 第 25〜27 弾の probe 群も全て再確認。 fixture: `09_subtyping/nested_container_covariance.il`。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 27 弾**。 **map リテラルが宣言型に対して covariant でない** checker の不整合を修正 (ユーザー要望 = 「本来こうあるべき形」)。 `let m: Map<string, Animal> = {"a": new Dog()}` は配列の `[new Dog()]: Animal[]` が通るのに checker が「got Map<string, Dog>」で拒否していた (lowering には第22弾の `lower_map_literal_with_hint` があり宣言 K/V で構築できるのに、 mod.rs の `literal_assignable_with` が **MapLit を意図的に除外**し陳腐化したコメントを残していた)。 修正 2 点: (1) `literal_assignable_with` に MapLit arm を追加 ([mod.rs](../crates/ilang-types/src/checker/mod.rs)) — 各 key/value を宣言 K/V へ配列/タプルと同じ subtype + リテラル coerce で検査 (let / 引数 / return をカバー)。 (2) let 束縛が `Map<K,V>` 注釈付きリテラルを新設 `check_map_lit_with_hint` ([casts.rs](../crates/ilang-types/src/checker/expr/casts.rs)) 経由で検査 ([stmt.rs](../crates/ilang-types/src/checker/stmt.rs)、 配列の `check_array_with_hint` と対称) — `{"a": new Dog(), "b": none}` のような some(child)/none 混在を親型へ unify (推論経路は Dog と none を unify 不能)。 **covariance はリテラル限定で健全**: 別名 `Map<string, Dog>` 変数は依然 `Map<string, Animal>` へ代入不可 (親型越しの mutation が unsound)。 全位置 (let/arg/return) で値・ARC 厳密一致 (deinit 700 + churn delta=0)・Optional 越しの仮想ディスパッチ正常を確認。 fixture: `09_subtyping/map_literal_covariant_value.il` (homo/optWrap/mixed/arg/return を pin)、 `09_subtyping/map_literal_covariant_value_alias_error.il` (別名拒否を expect-error で pin)。 syntax.md / syntax_ja.md の暗黙変換表を更新。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 26 弾**。 **subclass を `Optional<Parent>` へ包む `T → T?` 自動 wrap が lowering で全位置未対応** だった既存バグを probe で検出・修正 (`inner == source` の**厳密一致**を 3 箇所が open-code していたため、 `Dog` を `Animal?` slot へ流すと wrap されなかった)。 症状: (1) `coerce` (let / 引数 / return / 再代入 / tuple リテラル / 配列リテラル) は checker が通すのに lower 時「no coercion from Dog to Animal」で停止。 (2) Map index 代入 / 配列 index 代入 / **field 代入** は生オブジェクトを Optional slot に格納し、 解放時に Optional kind で誤カスケードして不整列ポインタ参照で **SIGABRT**。 (3) `release_owned_wrap_source` も wrap を見落とし、 値が正しい経路でも fresh 源の +1 を leak。 修正: wrap 述語を 3 箇所 — `coerce` の Optional wrap arm ([coerce.rs](../crates/ilang-mir/src/lower/coerce.rs))・`release_owned_wrap_source` ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs))・`AssignField` の `needs_optional_wrap` ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)) — で「plain `Object → Optional<Object>`」(subclass) を厳密一致に追加。 `Optional<_>` 源は Optional→Optional widen のため除外 (`none` / `Dog? → Animal?` は不変)。 同族探索: let / return / 再代入 / tuple / 配列リテラル / 配列 index / Map index / field / enum payload / borrowed 源 の全 11 位置で deinit 厳密一致・churn delta=0、 Optional 越しの仮想ディスパッチ (Dog.val() = n*10) も正しいことを確認。 fixture: `09_subtyping/subclass_optional_wrap.il` (11 位置 + none/widen 回帰ガードを厳密 deinit 1200 + byte delta=0 で pin)。 **別件として記録**: `let m: Map<string, Animal?> = {"a": new Dog()}` は **checker** が `Map<string, Dog>` を `Map<string, Animal?>` に推論できず明示拒否 (lowering 以前の型推論ギャップ。 crash/leak ではない — 対応するなら map リテラル値の subclass+Optional 統合)。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 25 弾**。 **`T → T?` / `T → T.weak` 暗黙 wrap coerce の所有元解放が「コンテナ store 位置」4 箇所で欠落** していた既存バグを probe で検出・修正 (第 20〜22 弾の同族で未踏だった配置)。 第 20〜22 弾は let / 引数 / return / 再代入 / field 代入 / Map リテラルを `release_owned_wrap_source` (または `lower_arg_to`) 経由に統一したが、 以下 4 つは独自に coerce + retain を open-code し wrap 源の +1 を落としていなかった: (1) **`m[k] = box`** (Map index 代入 against `Map<_, Box?>`) — **そもそも coerce 不在**で生 Box を Optional として格納 → `$map.release` が Optional kind で誤カスケード解放し不整列ポインタ参照で **SIGABRT** (最も深刻)。 (2) **`arr[i] = box`** (配列 index 代入 against `Box?[]`) — coerce はするが源 +1 を leak (fresh) / wrapper セルを二重計上 (borrowed)。 (3) **`[box]: Box?[]`** (配列リテラル要素)・(4) **`(box, _): (Box?, _)`** (タプルリテラル要素) — 同型 leak。 修正: 各 store サイトが wrap (`coerced != source`) を検出して aliased-element Retain の代わりに `release_owned_wrap_source` を呼ぶ ([expr.rs](../crates/ilang-mir/src/lower/expr.rs) AssignIndex の Array/Map arm、 [literals.rs](../crates/ilang-mir/src/lower/literals.rs) の `lower_array_literal_with_hint` / `lower_tuple_literal_with_hint`)。 Map arm は宣言 V へ coerce してから格納し、 host_map_set の retain 後に残る transient セルも解放。 同族探索で問題なし確認: `m.set(k, box)` (lower_arg_to 経由)・明示 `some(..)`・enum payload wrap・field 代入 (第 22 弾) は元から健全、 return が配列リテラルを返す形・入れ子 (tuple 内 `Box?[]`)・weak のコンテナリテラル/index 代入も修正経路で正しく解放。 fixture: `05_edge_cases/wrap_coerce_container_stores.il` (4 バグ + 健全経路の回帰ガードを厳密 deinit 1000 + churn delta=0 で pin)。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 24 弾**。 第 23 弾で入れた Map/Set 挿入順機構 (`order: Vec<i64>`、非所有ハンドル) の **ARC 次元**を厳密 deinit + churn delta=0 で網羅 — **新規バグなし** (クリーン 4 回目)。 probe: 文字列キー Map の上書き+delete+再挿入 (3 deinit/round)、 object キー Map (delete が key share を落とす・2/round)、 object 要素 Set の union/intersection/difference (eq 等価で別アロケーション混在 — survivor の二重 retain も早期解放も無し・4/round)、 forEach 反復中の現在キー delete (snapshot の +1 が値を callback 終了まで生存させる・3/round)、 Optional 値の上書き+delete カスケード、 文字列 Set union の順序+ARC。 全 probe で deinit 厳密一致・churn 100 周 delta=0。 挿入順意味論 (上書き=位置維持 / delete+再挿入=末尾) と第 23 弾で直したプロセス間決定性 (5 回実行で同一順序) も再確認。 fixture: `03_collections/map_set_order_arc_churn.il` (上記 4 形を厳密 deinit + byte delta=0 で pin)。 fixture 追加のみ (ソース変更なし) のため workspace 全体と nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認 (PASS、 byte delta=0 が両経路で成立)。
- **第 23 弾**。 breadth probe (entries 反復・@derive Set churn・weak 昇格・template×join・field 格納 closure・Promise.all・継承・脱出 closure) は **ARC 新規バグなし** (クリーン 3 回目、 churn 1300 まで正確)。 代わりに **Map / Set の反復順がプロセスごとに非決定** という言語仕様レベルの問題を検出 (同じプログラムで entries() の順序が実行ごとに変わる — Rust HashMap の per-process SipHash シード)。 **ユーザー決定: JS と同じ挿入順保証**。 `ManagedMap` / `ManagedSet` に非所有の挿入順リスト `order: Vec<i64>` を追加 (handle = Int は生値 / Str は初回挿入の orig ポインタ / Object は格納ポインタ)、 keys/values/entries/forEach/印字/集合演算 (union は第1集合→第2集合の順) を order 走査に統一。 上書きは位置維持・delete+再挿入は末尾 (JS 同様)。 印字の「文字列ソートで安定化」は挿入順表示に置換 (数値キーが `1, 2, 10` と自然に並ぶ)。 fixture: `03_collections/map_set_insertion_order.il` (Map/Set/object 要素/float 要素/集合演算)、 console_log_map.il と第 23 弾 breadth fixture の期待値を挿入順に更新。 order 機構の leak 無しは 200 周 churn (delta=0・deinit 1200 正確) で確認。 検証: nextest 539/539、 AOT PASS、 nested_generic 50 並列 0 fail。
- **第 22 弾**。 wrap coerce × セル格納の組合せを probe して **既存バグ 4 系統を検出・修正**: (1) `h.opt = new Box(1)` (field 型 `Box?`) — AssignField 自前の wrap coerce に所有元解放が無い (第 21 弾の 5 箇所に続く 6 箇所目。 strong→weak field も同時に修正)。 (2) `takeOpt(h.b)` — **借用ソース**の wrap が引数位置で作る fresh セルを post-release 判定が見ない (判定が元式の freshness のみ) — `last_arg_wrapped` フラグを `lower_arg_to` が立て、 5 つの呼び出し形 (fn / method / init / extern / variadic) の判定に OR。 (3) `let o: Box? = h.b` / ブロック式ソース — bind retain が元式の freshness で判定し、 **wrap が作った fresh セルを二重 retain** して leak (Assign も同型) — coerce が新値を作ったら fresh 扱いに。 (4) `let m: Map<string, Box?> = {"a": none, ...}` — **checker が通すのに lowering が落ちる不整合** (「no coercion from Map<string, ()?>」) — `lower_composite_with_hint` に MapLit arm が無かった。 `lower_map_literal_with_hint` 新設 (K/V を宣言型で構築、 none が V を採用、 wrap・nested hint・transient 解放込み)。 問題なし確認: property setter への wrap、 enum payload `Box?`、 `Box?[]` push、 return 位置の借用 wrap。 fixture: `05_edge_cases/wrap_cells_and_hinted_map.il`。 検証: nextest 539/539、 AOT PASS、 nested_generic 100 並列 0 fail。
- **第 21 弾**。 **`T → T?` / `T → T.weak` 包み coerce の所有元解放が let 以外の全位置で欠落** していたのを probe で検出・修正 (1 個/call leak): weak の join (`let w: Box.weak = if c { a } else { b }` — join 正規化が立てた強 +1 が weak 束縛に流れて行き場なし)・引数位置・代入・return 両経路 (末尾式 / 明示 `return`)。 修正は共通ヘルパー `release_owned_wrap_source` (所有判定 = fresh ∨ block tail retain 済み) を let / `lower_arg_to` / Assign / `lower_return` / `finalise_return` (tail_owned を呼び出し元 4 箇所から供給) に適用。 fixture: `05_edge_cases/wrap_coerce_fresh_release.il`。 **既知の制限を記録**: async fn 内の Result `?` は checker で型エラーになる (「expected Promise<Result<...>>, got Result<any, string>」という分かりにくい文言 — Optional の async 診断と違い専用メッセージなし。 対応するなら ? の async 対応一式とセット)。 join を return / break / 文破棄 / 入れ子 / field 代入 / tuple / 文字列で churn する probe は全て正確 (1300/449 まで一致)。 検証: nextest 539/539、 AOT PASS、 nested_generic 100 並列 0 fail。
- **第 20 弾**。 **混合 freshness の join が片側を 1 個/評価 leak する一般バグ** (if / match / if let すべて、 `?` 以前からの既存) を probe で検出・修正。 `if c { new Box(1) } else { fallback }` のような「片腕 fresh・片腕借用」の join は freshness 述語が「全腕 fresh」を要求するため非 fresh 扱いになり、 消費側の retain が fresh 腕を過剰計上していた。 修正は **join の所有正規化**: `lower_block_hinted` が tail retain を発行したかを `last_block_tail_owned` で公開し、 if / match (optional / enum / int / bool / str) / if-let の全 join (11 箇所) で「非所有の heap 結果に join 側 Retain」を入れ、 join 値は一律 +1 所有に。 これにより `is_fresh_object_expr` の If / Match / IfLet 規則は複雑な分岐 (arm_returns_own_binding 等) ごと `true` に単純化・削除。 join coerce が新セルを作る場合 (T→T? 包み / fixed copy) は所有元の +1 を解放。 併せて **fresh 引数の T→T? 自動包み leak** (`takeOpt(new Box(42))` — stmt-let 側は前ラウンドで修正済みだった残り) を `lower_arg_to` に同型の解放で修正。 fixture: `05_edge_cases/mixed_freshness_join.il` (if/match/iflet × deinit 数 + churn delta=0)。 probe 済みで問題なし: `?` の全配置 (引数位置・束縛経由・loop 内・文として破棄・arm 内・連鎖)、 固定長配列の新経路の組合せ (fresh 戻り値→push/map.set/fill、 for-in 早期 return、 Optional<Box[2]> field、 Map values() 反復、 再代入×所有者先死、 ?×固定長 payload) — churn 1150 まで正確。 検証: nextest 539/539、 AOT PASS、 nested_generic 100 並列 0 fail。
- **第 19 弾**。 ユーザー決定 (a) を受けて **固定長配列 × heap 要素を正式サポート**: fresh リテラル束縛が所有・エイリアスは借用・field 代入は fresh=ポインタ転送 / 非 fresh=値コピー (`$array.copyFixed`)・drop cascade は合成タグ `KIND_FIXED_BASE + len*16 + elem_kind`。 置き場所は field / ローカル / param に checker で限定 (戻り値・コンテナ構成要素・capture・再代入は型エラー)。 fixture 2 件 (包括 pin + placement エラー)。 詳細は下の解決済み記録。

それ以前のセッション (2026-06-10、 ARC ラウンド) で main に landing した変更:

- **fixture 増殖ラウンド** (`480ed47a`〜`ac91f68b`、 後続セッション)。 leak / 別名健全性の probe を Map / Set / Array / Optional / weak / template literal へ網羅的に当てて 5 系統のバグを検出・修正。 回帰 fixture 8 件追加。 詳細は下の解決済み記録「fixture 増殖ラウンドで検出した 5 系統」:
  - `arc_peephole` が「他値の Release は跨いで安全」と誤判定 → `makeMap()["k"].n` が解放済みメモリを読む (**正しさバグ**、 `480ed47a`)
  - `__release_object` の field cascade 中に weak back-ref の `__release_weak` が本体を解放 → 二重解放 SIGSEGV (**既存の潜在バグ**、 `d3b1d2cf`)
  - 配列 `indexOf` / `includes` / `remove` が string をポインタ比較 → 実行時生成文字列で不一致 (**正しさバグ**、 `ebb95b4a`)
  - Map/Set の `get`/`has`/`delete` fresh needle 引数、 fresh receiver (string/array/Optional のメソッド・`.length`・`.isSome`) の transient +1 が release されず leak (`8b9f8c31`)
  - template literal が評価ごとに registry string を 2+ 個 leak (`aace25c7`)
- **`m[k]` の Map index 読みを `ArrayLoad` と同じ borrow 規約に統一** (`f3d0a899`、 後続セッション)。 `__map_get` の retain-on-read (`d8c7f548`) と borrow 前提の消費側 (束縛 Retain / tail-borrow Retain / arc_peephole の whitelist) が二重 retain になり、 overwrite された entry が 24 bytes/iter で leak していたのを解消。 詳細は下の解決済み記録。
- **cranelift 0.131 → 0.132.1 へ依存上げ** (`12d171d4`)。 API breaking なし。 nested_generic.il race の調査用に試行したが race 確率は不変 (cranelift_module 側に修正は入っていない)。 修正とは独立で温存。
- **class method body の bare-var tail を borrow として扱う** (`4e4e6851`)。 [crates/ilang-mir/src/lower/body_cx.rs::lower_block_hinted](crates/ilang-mir/src/lower/body_cx.rs:695) の `tail_is_borrow` が `Index/Field` のみ matching していたところに、 `Var(name)` でも「class method 内 + env/capture 未解決」 なら暗黙の `this.field` として retain を発行するよう拡張。 `nested_generic.il` の race を根本解決。 同時に dd40bc49 の `forget(compiled)` も撤去。
- **property getter を bare-var borrow retain の対象外に** (`ac6e787e`)。 method 用の retain は property access の caller-side retain と二重になるため、 `BodyCx::is_property_getter` を追加して getter body だけ skip。 200 iter で 24 bytes/iter leak していた回帰を解消。
- **closure body 内の bare-var field を `LoadCapture` 経由で解決** (`a0c2a854`)。 [crates/ilang-mir/src/lower/collect.rs](crates/ilang-mir/src/lower/collect.rs) の `Var` arm で free var に `this` を candidate に追加 + [crates/ilang-mir/src/lower/expr.rs::lower_var_expr](crates/ilang-mir/src/lower/expr.rs) の field path で env に `this` がなければ captures からフォールバック。 `class Holder { inner: Box; grab(): fn(): Box { fn(): Box { inner } } }` で `lower_var_expr` の `Option::unwrap()` panic していたのを解消。
- **borrow-tail retain を fn-body の outermost block でだけ発火** (`775c6026`)。 [crates/ilang-mir/src/lower/body_cx.rs](crates/ilang-mir/src/lower/body_cx.rs) に `in_fn_body_top` flag + `lower_block_for_fn_body` wrapper を追加。 if-arm / match-arm / loop-body の sub-block では retain skip。 `pick(): Box { if flag { a } else { b } }` で 24 bytes/iter leak していたのを解消。
- **top-level let が class field 名を hijack する shadow bug を解消** (`a78bfd2a`)。 [crates/ilang-mir/src/lower/expr.rs::lower_var_expr](crates/ilang-mir/src/lower/expr.rs) で `repl_slots` lookup より先に implicit `this.field` を解決する順序に変更 (= OOP の「class members shadow globals」 ルール)。 `let base = test.liveAllocBytes()` で `Forge.base` field が誤って REPL slot 経由になり、 200 iter ループ内で `test.expect(g().n, i)` が常に 0 を返す wrong-value bug を解消。

regression fixture 9 件 (`05_edge_cases/method_tail_bare_var_if_arm.il`、 `05_edge_cases/method_tail_bare_var_match_arm.il`、 `05_edge_cases/method_tail_match_enum_payload.il`、 `05_edge_cases/method_tail_bare_top_level_fn.il`、 `05_edge_cases/method_field_shadows_top_level_let.il`、 `08_properties/getter_tail_bare_var_heap.il`、 `09_subtyping/method_tail_bare_var_parent_field.il`、 `10_closures_arc/closure_tail_bare_var_field.il`、 `059303f5` の match-arm/payload 2 件) を追加。

それ以前に同セッションで landing 済み (sret ラウンド):

- **内部 fn の CRepr struct return を sret 経路に倒す** (`4d1f97dc`)。 `crepr_struct_field_discard.il` の leak (= chunks return で callee の `new Box()` buffer が宙吊り) を塞いだ。 `Terminator::Return` に `release_value: bool` を追加し、 codegen が sret memcpy 後に callee 側 buffer を `__mir_free` する。 `is_c_abi` (= `Extern { .. } | ExternBody`) は従来の platform chunk → HFA → sret cascade を維持して SDL2 / wgpu / objc_msgSend を守る。
- **`Inst::VirtCall` も同じ sret 経路に統一** (本コミット)。 `call_dispatch.rs::VirtCall` が `struct_indirect_with_max` のままだったため、 vtable 経由で 16 byte 以下の CRepr struct (NSRange / NSRect 等) を返す `@objc method` の caller signature (chunks return) と callee signature (sret) が決定的にミスマッチし、 debug build で SIGSEGV を踏んでいた (`cocoa_foundation/calendar_test.il`、 `cocoa_appkit/drawing_test.il`)。 vtable に乗るのは構造的に `FunctionKind::Local` のみなので `struct_sret_for_internal` に統一すれば整合する。
- **CRepr struct の inline enum field を表す `MirTy::CReprEnum` を導入** (`28f7060f` → `65bb326a` → `14292c5e`、 前セッション)。
- **`match` / `if let` のアームバインディング tail-Var Retain** を `Binding::PatternBinding(_, _, needs_retain_on_tail)` で表現し直し (`ef1b9d35` → `838d2dc4`、 前セッション)。
- **closure body 内 cell store の rc** を 2 path に分離 (`50eb400a` + `46feb093`、 前セッション)。
- **`Binding::Ssa` 細分化と rc-slot 集約** (`4afd282e` → `d6b2e64f` → `838d2dc4`、 前セッション)。
- **CRepr fresh return の leak 調査用に `ILANG_HEAP_TRACE` env を追加** (`bcd3367f`、 前セッション)。

次のフェーズ候補: **capability の enforce** (`@requires` はパース済み・未 enforce)、 **未実装の言語機能 (Iterator プロトコル、 `?` の Optional 対応など — タプルと Result 用 `?` は実装済みと第 15 弾で確認)**、 **C ヘッダから .il 自動生成のミニ bindgen**、 **REPL の `use` 対応 (loader overlay 方式の素案は第 15 弾の記録参照)**。

## 未解決の引き継ぎ事項

### [解決済み記録] 第 162 弾: chained getter `a.b.getter` がレシーバと leaf を二重リーク (2026-06-19)

最古の fixture 領域 `08_properties`(2026-06-13 以降未更新)を probe して検出。 **property getter をレシーバが field/property アクセスの形で読む**(`o.inner.boxed`、 一般的な `a.b.getter`)とリークする。

- **症状**: `o.inner.boxed.n`(`inner` は getter、 `boxed` は fresh Box を返す getter)を churn すると **leaf の Box が解放されない**(delta=0)。 `boxed` が borrowed を返す場合・leaf が plain field/scalar の場合は **中間の Inner が解放されない**(`o.inner` の +1 が漏れる)。 切り分け: `let inr = o.inner; inr.boxed.n`(分解)・method チェーン `o.inner().boxed()`・Var レシーバ `inr.boxed.n` は全て健全 → **chained Field レシーバ固有**。
- **原因 (1) leaf の owned 判定漏れ**([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs) `field_is_property_access`): getter 読みが owned(+1)かは「`name` が getter かつ obj のクラスが解決できる」で判定するが、 obj を `this`/Var/クラス名しか解決していなかった。 chained Field(`o.inner`)は `_ => None` で **borrow 既定**に落ち、 leaf getter の +1 が consumer に drop されず leak(コメントに「chained reads は leak、 never UAF」と既知制限として明記)。
- **原因 (2) fresh レシーバの解放漏れ**([literals.rs](../crates/ilang-mir/src/lower/literals.rs) getter dispatch): getter call は受け手 `ov` を borrow するだけだが、 `ov` が fresh(`o.inner` や `mkOuter().prop`)のとき call 後に Release していなかった。 直下の `.length` 経路は `obj_is_fresh && is_arc_heap` で解放しているのに getter 経路に欠落。 中間 getter 結果が 1 read につき 1 個 leak。
- **修正**: (1) 再帰ヘルパ `resolve_static_class_id(obj)` を新設 — `this`/Var に加え **`Field{obj, name}` を再帰解決**(member の getter 戻り型 or field 型が Object ならそのクラス)。 `field_is_property_access` の instance 解決をこれに置換し、 chained getter を owned 判定。 (2) getter dispatch に `if obj_is_fresh && self.is_arc_heap(&oty) { Release(ov) }` を追加(`.length` 経路と同形)。
- **同族確認(§8-6)**: plain field 読みの fresh レシーバ解放(`mk().n`・`f.make().n`・`o.inner.tag`)は元から健全。 method チェーンも健全。 穴は getter dispatch 固有だった。
- **fixture**: `08_properties/chained_getter_receiver_arc.il`(leaf fresh getter + 中間 getter レシーバ、 Inner/Box を別々に計数し churn 各 100、 `ILANG_HEAP_GUARD=1` クリーン)。
- **検証**: ilang-mir 58/58、 programs JIT PASS、 programs AOT 1298/1298 PASS、 nested_generic 100/100。

### [解決済み記録] 第 161 弾: 第 159 弾の取りこぼし — 代入 RHS と cast の発散式が codegen を panic させる (2026-06-19)

第 159 弾(発散式を消費位置で拒否)の **未カバー位置**を網羅確認(§8-6)して検出。

- **症状**: `x = return 5`(代入)→ `panic` at locals.rs:39、 `c.n = return 5`(field 代入)→ `panic` at objects.rs:781、 `a[0] = return 5`(index 代入)→ `panic` at array.rs:347 と **MIR codegen が Rust panic**(第 155〜158 の verifier エラーより重い ICE)。 `(return 5) as i64`(cast)は `mir lower: no coercion from () to i64`。 第 159 弾は二項/単項・call 引数・配列/タプル/Map 要素・index 読み・some・template は塞いだが、 **代入の RHS 系統と cast オペランドが穴**だった。
- **原因**: 代入 RHS / cast オペランドも値消費位置だが、 `return X` の checker 型が関数戻り値型のため `value_assignable` を通過し、 lowering で発散式のプレースホルダ(`()`)を実値として扱い panic / coerce 失敗。
- **修正**: `reject_control_transfer_value`(第 159 弾で新設)を残りの消費位置にも配置 — `ExprKind::Assign` の value([checker/expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs))・`check_assign_field` の value・`check_assign_index` の obj/index/value([access.rs](../crates/ilang-types/src/checker/expr/access.rs))・`check_cast` のオペランド([casts.rs](../crates/ilang-types/src/checker/expr/casts.rs))。
- **対象外(既にクリーン拒否)**: field/method レシーバ(`(return 5).n` → 型不一致 `expected <object>`)、 `if`/`while` 条件(`expected bool`)、 await(`expected Promise`)は `return` の型(関数戻り値型)が要求型と合わず既存の Mismatch 診断で弾かれる。 let RHS(`let x = return 5`)は return が即発火し x が dead なので無害(正常終了)。
- **fixture**: `05_edge_cases/control_transfer_in_assign_rhs.il`(`x = return 5` を expect-error で pin、 ヘッダに field/index/cast も記載)。
- **検証**: ilang-types 76/76、 programs JIT PASS、 programs AOT 1298/1298 PASS。 checker のみ・lowering 不変。

### [確認済み記録] 第 160 弾: overloading 選択 × heap ARC — 全て健全 (2026-06-19)

「最近 fixture 化されていない領域」を BUG_COVERAGE の追加日集計で特定し、 最古の **06_overloading / 07_method_overloading**(共に 2026-06-07 以降未更新)を probe。 **新規バグなし。**

- **確認した形**: free-fn overload を heap 引数(`f(Box)` vs `f(i64)`)・subclass(`label(Animal)` vs `label(Dog)` に `Dog` を渡すと Dog 版選択、 `Animal` 型変数だと Animal 版)・heap 返却(`make(i64)`/`make(string)` が `Box` 返却)・`T` vs `T?`(exact 優先)で選択し、 init/method overload(`init(Box)`/`init(Box,Box)`、 `add(Box)`/`add(Box,Box)`)に heap 引数を渡す。 全て選択正当 + 選んだ overload が heap を過不足なく 1 回 deinit(churn 厳密=300 / 600・`ILANG_HEAP_GUARD=1` クリーン)。
- **既存 fixture との差**: 既存 overloading fixture は scoring / 選択の正しさのみで **ARC が未検証**だった。 オーバーロード dispatch 経路が heap 引数/戻り値の retain/release を壊さないことを pin。
- **副次確認**: 第 107 弾が「壊れている既知の制限」と記録した **implicit-this generic メソッド呼び出し**と **generic 返却メソッド**は現在正常動作する(`generic_method_this_call.il` / `generic_method_returns_generic.il` で pin 済み)。 第 107 弾の当該記述は古い。 明示型引数 `id<i64>(5)` は依然 parser 未対応(`<` を比較とパースし `undefined variable "i64"`、 graceful)。
- **fixture**: `06_overloading/overload_selection_heap_arc.il`・`07_method_overloading/init_method_overload_heap_arc.il`。
- **検証**: programs JIT PASS、 新 fixture を JIT/AOT で確認(コード変更なしのため nested_generic 儀式は非対象)。

### [解決済み記録] 第 159 弾: 発散式を値消費位置に置くと内部的な lower エラー → checker で拒否 (2026-06-19) — ユーザー決定

155〜158 の発散系統を掃いた後、 **発散式を「値として消費する位置」**(値 join でない所)に置いた場合を probe して検出。

- **症状**: `f(return 5)`・`1 + (return 2)`・`[1, return 2, 3]` が checker を通り、 `mir lower: no coercion from () to i64` / `cannot unify i64 and ()` という内部的な見た目のメッセージで exit 1(パニックや verifier クラッシュではない graceful なエラー)。
- **原因**([checker/expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs) `ExprKind::Return`): `return X` の checker 型を **関数の戻り値型**(`Ok(expected)`)にしている — これは `fn f(): T { return X }` や `if c { return X }` を never 型なしで型検査するための意図的設計。 そのため `f(return 5)`(引数位置)等も型が合致して素通りし、 MIR lowering で発散式のプレースホルダ `()` を要求型へ coerce できず落ちる。 `todo() + 1`(別途保留)と同じ「never 型不在」が根。
- **判断**: never 型導入(Rust 風に coerce で通す)は型システムへの本格追加で `Any`/`Unit` との相互作用検証が大きい。 ユーザー決定で **never 型は入れず、 消費位置で checker が綺麗な診断で拒否**。
- **修正**: `reject_control_transfer_value(e, loop_depth, ret_ty)` を新設(`pub(in crate::checker)`)。 値消費位置で呼ぶ: 二項/単項オペランド・自由関数 call 引数(`check_call_expr`)・メソッド/ctor/overload 引数(`check_args`)・配列要素(`check_array_with_hint`)・タプル要素・Map キー/値(`check_map_lit_with_hint`)・index(`check_index`)・`some(..)`・template parts。 **well-formed な control-transfer のみ拒否**: `break`/`continue` は `loop_depth>0` の時だけ(ループ外は既存の「used outside of a loop」に譲る)、 `return` は `ret_ty.is_some()` の時だけ(トップレベル return の専用エラーに譲る)。 これで `break_outside_loop_error.il`/`continue_outside_loop_error.il` の期待メッセージを保つ。
- **fixture**: `05_edge_cases/control_transfer_not_a_value.il`(`g(return 5)` を expect-error で pin)。
- **残課題**: `todo() + 1`(`todo()` は `Type::Any` でオペランド位置の演算を受けない)は同系統だが別件で保留(優先度低)。
- **検証**: ilang-types 76/76、 programs JIT PASS、 programs AOT 1298/1298 PASS。 checker のみ・lowering 不変のため nested_generic 儀式は非対象。

### [解決済み記録] 第 158 弾: if 式の発散分岐が MIR codegen を verifier エラーで落とす (2026-06-19)

第 157 弾(int/bool match の発散 arm)の同型構造を **if 式の値 join** で BUG_COVERAGE.md を見ながら probe して検出。 既存バグ。

- **症状**: `fn f(c: bool): i64 { let x = if c { 9 } else { return 1 }; x + 100 }` が **`mismatched argument count for jump block4: got 0, expected 1`** でクラッシュ。 then 発散/else 発散の双方向、 heap 値(`if c { new Box(1) } else { return new Box(2) }`)、 elif 中段の発散、 引数位置の if(`g(if c { 9 } else { return 1 })`)でも同様。 `if c { return 1 } else { 7 }` は `cannot unify () and i64` の型風エラー(同根)。
- **原因**([control.rs](../crates/ilang-mir/src/lower/control.rs) `lower_if`): join 型を then/else 両分岐の tail から選び(発散分岐は body lowering でプレースホルダ `MirTy::Unit` を返す)、 両分岐とも `cont` へ `Br` を発行。 発散分岐の死にブロックが `br cont(())` を出し、 live 分岐の型(i64/obj)を持つ join param と clif で arity 不一致(Unit=0 幅)。 match と同じく `lower_if` は発散分岐を一度も特別扱いしていなかった **既存バグ**(第 155〜157 とは独立)。
- **修正**: `block_diverges`(then は AstBlock)/`arm_body_diverges`(else は Expr)で各分岐の発散を判定。 (1) join 型(`result_ty`)は **live 分岐のみ**から選ぶ(片方発散ならもう片方の型、 両方発散なら Unit)。 (2) 発散分岐の末尾ブロックは join へジャンプせず `Terminator::Unreachable` で閉じる。 `block_diverges` を `pub(super)` 化して control.rs から使用。
- **fixture**: `05_edge_cases/if_diverging_branch_value.il`(else 発散 / then 発散 / heap else 発散 / 発散経路 100 回 churn deinits=100・`ILANG_HEAP_GUARD=1` クリーン)。
- **検証**: ilang-types + ilang-mir 134/134、 programs JIT PASS、 programs AOT 1298/1298 PASS、 nested_generic 100/100。

### [解決済み記録] 第 157 弾: int/bool match の発散 arm が MIR codegen を verifier エラーで落とす (2026-06-19)

第 156 弾(発散 break)の隣接面「発散 × match」を BUG_COVERAGE.md を見ながら probe して検出。 **整数・bool の `match` に発散 arm がある**と codegen がクラッシュする既存バグ。

- **症状**: `fn f(s: i64): i64 { match s { 0 { 9 } _ { return 1 } } }` が **`mismatched argument count for jump block1: got 0, expected 1`** でクラッシュ(値を返す arm + 発散 arm の組合せ)。 heap 値・bool match でも同様(`match b { true { new Box(1) } false { return new Box(2) } }`)。 **string match は元から正しく動く**(差で原因が絞れた)。
- **原因**([match_.rs](../crates/ilang-mir/src/lower/match_.rs) `lower_match_int`/`lower_match_bool`): enum 経路(`lower_match_enum`)と string 経路(`lower_match_str`)は arm ごとに `arm_body_diverges` を見て発散 arm を join のリストから除外するが、 **int/bool 経路は全 arm を無条件で `joins.push`**。 発散 arm は body lowering(`return`/`todo`)で死にブロックを開き、 そこで得たプレースホルダ値(`MirTy::Unit`)を join へ渡す。 join param は値 arm の型(i64/obj)なので、 `br cont(())` が clif で 0 arg(Unit はゼロ幅)対 1 param の arity 不一致。
- **新規/既存**: int/bool 経路は導入以来 `arm_body_diverges` を一度も参照していない **既存バグ**(第 155/156 の checker・break 修正とは独立)。 `match` の arm に `return` を書きつつ別 arm が値を返す形が fixture に無く、 見逃されていた。
- **修正**: `lower_match_int` の 3 arm 種別(Wildcard / IntLit / IntRange)と `lower_match_bool` の 2 arm で、 body lowering 前に `arm_body_diverges(&arm.body)` を取り、 発散 arm は `ensure_join_owned`/`result_ty` 更新/`joins.push` を全てスキップ(string 経路と同形、 死にブロックの明示終端は不要 — codegen が許容)。
- **fixture**: `05_edge_cases/match_int_bool_diverging_arm.il`(int scalar の取らない/取る両経路・int heap arm・bool heap arm・発散経路 100 回 churn で deinit 厳密=100・`ILANG_HEAP_GUARD=1` クリーン)。
- **検証**: ilang-types + ilang-mir 134/134、 programs JIT PASS、 programs AOT 1298/1298 PASS、 nested_generic 100/100。

### [解決済み記録] 第 156 弾: 発散値を持つ break が MIR codegen を verifier エラーで落とす (2026-06-19)

第 155 弾(`break todo()` の checker 修正)の隣接面を BUG_COVERAGE.md を見ながら probe して検出。 break の**値自体が発散**する形が codegen を壊す。

- **症状**: `loop { if c { break (return 1) } break 8 }` が **`mir-codegen: Verifier(... mismatched argument count for jump block2(v5): got 1, expected 0)`** でクラッシュ(型エラーでなく不正 IR)。 `break (if c { return 1 } else { return 2 })` も同様。 `break (return 1)` 単独が最小再現。
- **原因**([control.rs](../crates/ilang-mir/src/lower/control.rs) `lower_break`): break 値が発散しても他の値 break と同じく lower し、 発散値の lower 結果(プレースホルダ `MirTy::Unit`)で loop exit ブロックに param を遅延追加。 exit param 型が `()` に確定し、 後続の実値 break(`break 8`)が `br exit(i64)` で渡すと Unit(=clif 0 param)と arity 不一致。 checker は第 155 弾で発散 break を join から除外済みだが MIR が追随していなかった。 加えて MIR の `arm_body_diverges`([match_.rs](../crates/ilang-mir/src/lower/match_.rs))が **`if`/`match` の発散を判定していなかった**ため、 `break (if all-return)` は発散と認識されず素通りしていた。
- **修正**: (1) `lower_break` の冒頭で、 break 値 `e` が `arm_body_diverges(e)` を満たすなら値を lower(return/abort の副作用を保つ)後、 現ブロックを `Terminator::Unreachable` で閉じて即 return — exit param 追加もジャンプ値も行わない(発散 break は exit へ到達しないため)。 (2) `arm_body_diverges` に「`else` を持ち then ブロックと else 式が共に発散する `if`」「arm を持ち全 arm が発散する `match`」を追加(`block_diverges` ヘルパを抽出)。 これで match arm 本体が発散 if/match の場合の lowering も同じ Unreachable 経路に乗る。 checker 側 `arm_body_diverges` は型が既に正しい(クラッシュは codegen のみ)ため変更せず。
- **fixture**: `05_edge_cases/loop_break_diverging_value_codegen.il`(`break (return v)`・`break (if all-return)` を実値 break と混在 + 発散経路を実際に通って fn が return する形)。
- **新規/既存**: `break <発散値>` は第 155 弾が checker を緩めて初めて codegen に到達したため、 155 が露呈させた MIR 側の穴(`todo()` 以前は書けない形)。
- **検証**: ilang-types + ilang-mir 134/134、 programs JIT PASS、 programs AOT 1298/1298 PASS、 nested_generic 100/100。

### [解決済み記録] 第 155 弾: `break todo()` が loop 型を Any に固定し実値 break と衝突 (2026-06-19)

第 154 弾(`todo()` 追加)の同族を BUG_COVERAGE.md で「`todo()` の未 probe 配置」を見ながら突いて検出。 `loop` 内で `break todo()` と実値の `break v` を混ぜるとコンパイルできない。

- **症状**: `loop { if c { break todo() } break 5 }` が `type mismatch: expected any, got i64`。 順序を入れ替えても(`break 5` → `break todo()`)`expected i64, got any`、 heap 値でも `expected any, got Box`。 `break todo()` が唯一の break なら(実値 break 無し)通る。
- **原因**([checker/expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs) の `ExprKind::Break`): break 値型を `LoopFrame::Loop(acc)` に積む際、 値が diverge するかを見ていなかった。 `break todo()` の値 `todo()` は `Type::Any` を返すため loop 型を Any に固定し、 後続の実値 break が unify 失敗。 第 154 弾は `todo()` の divergence を `arm_body_diverges` 経由で **match arm と if-join にだけ**配線し、 **`loop` の break 値 join は取り残し**ていた(「1 つ直して同族を見落とす」§8-6 の典型)。 `todo()` 以前は `break todo()` を書けないので 154 弾が持ち込んだ穴(HEAD 比較不要)。
- **修正**: `Break` 処理の `LoopFrame::Loop` 分岐に「値が `arm_body_diverges` を満たすなら acc に積まない」ガードを追加。 `break <diverging-expr>` は実際には break せず abort/return するので、 match の diverging arm と同様 break 値 join に寄与させない。 新しい意味論ではなく match/if と同じ規則の loop への適用。
- **fixture**: `05_edge_cases/loop_break_todo_diverges.il`(scalar `break 5` と heap `break new Box(7)` を `break todo()` と混在 + 100 回 churn で deinit 厳密=100・到達しない経路は実値を返す)。 到達 abort は既存 `todo_reached_panics.il` が pin 済み。
- **未判断で残した別件**: `todo() + 1` は `cannot apply binary op between any and i64` で拒否される。 Rust の `todo!()` は never 型で i64 へ coerce するが ilang の `todo()` は `Type::Any` でオペランド位置の演算を受けない。 never 型化するか Any を演算位置で特別扱いするかは意味論の選択(§2 停止条件)でユーザー判断待ち。 実用上 `todo()` 単体が主用途のため優先度低。
- **検証**: ilang-types 76/76、 programs JIT PASS、 programs AOT 1298/1298 PASS(152s)。 checker のみの変更で lowering は不変のため nested_generic 儀式は非対象。

### [解決済み記録] 第 153 弾: REPL の bare 式 echo が i64 のみ (2026-06-15) — ユーザー決定

REPL を probe して **bare 式の値表示が i64 のみ**(`42` は出るが `"hello"`/`true`/`3.14`/配列等は無出力)を検出。 ユーザー判断で「全型 auto-print に改善」を選択。

- **原因**([main.rs](../crates/ilang-cli/src/main.rs) `run_chunk`): チャンク結果は `__main` の **i64 戻り値 `r`** を表示していた(`if r != 0 { r.to_string() }`)。 bare 式が i64 のときだけ tail 値が `r` に乗るので、 他型は `r=0` で何も出ない。 REPL テストは全て明示 `console.log(...)` を使い auto-print に非依存だったため見逃されていた。
- **修正**: `run` と同じ `wrap_trailing_print`(tail を `console.log(tail)` で包む)を REPL チャンクにも適用し、 i64 echo を撤去。 console.log は全型を整形表示。 **二重 print 回避**: `console.log(x)` tail は `console.log(console.log(x))` になるが、 内側が Unit を返し `console.log(())` は何も出さないので二重にならない(`run` で実証済み)。 statement-only チャンク(`let x=5`)は tail 無しなので無出力。 `run_repl` は元々空出力を抑制済み。
- **検証**: REPL テスト 2 件追加(`repl_bare_expr_echoes_all_types`・`repl_console_log_tail_not_doubled`)。 既存 14 件回帰なし、 workspace 548/548。 REPL は JIT 専用・`run_chunk` 局所変更なので programs harness/AOT は非対象。

### [解決済み記録] 第 152 弾: std.events の emit 中リスナ削除で index out of bounds (2026-06-15)

薄い領域 std.events([events.il](../../libs/std/events.il)・fixture 1 件)を probe して **emit 中にリスナを `off`/`remove` すると `panic: index out of bounds`** を検出・修正。

- **原因**: `EventEmitter.emit`/`Signal.emit` が `n = arr.length` を先に取り、 **生のリスナ配列を `arr[i]` で反復**。 リスナが emit 中に `off`(= `arr.remove`)を呼ぶと配列が縮み、 cached `n` のままなので次の `arr[i]` が範囲外 → panic。 古典的な concurrent-modification バグ。
- **修正**: 両 emit で発火前に **スナップショット**(`arr.slice(0, arr.length)`)を取り、 それを反復。 emit 中の追加/削除は当該 emit の走査を壊さない(Node.js 準拠: 発火対象は emit 開始時点の集合・削除は次回 emit に反映)。
- **クリーン確認**: 基本 on/emit/off・listenerCount・emit 中 on(追加は当該 emit で未発火)・removeAllListeners(ARC で配列生存)・空 emit は健全。 HEAP_GUARD 破損なし。
- **検証**: fixture `04_modules/events_emit_reentrancy.il`。 programs JIT+AOT、 workspace 546/546。 `events.il` も `include_str!` 埋め込みなので再ビルド必須。

### [解決済み記録] 第 151 弾: std.math の `sign(±0)` と `min`/`max` の NaN (2026-06-15)

バグ抽出が薄い領域(fixture 2 件・HANDOFF 言及 0)として **std.math** を probe し、 2 件の実バグを検出・修正。

- **`sign(±0.0)` が `1.0`**: doc は「`0.0` when `±0.0`」(JS `Math.sign` 準拠)と明記しているのに、 intrinsic が `f64::signum`(Rust の signum は `+0.0→1.0`・`-0.0→-1.0`)にマップされ `sign(0.0)=1.0` を返していた = **doc と実装の矛盾**。 修正([math.rs](../crates/ilang-runtime/src/math.rs)): `math_sign` を専用実装に(NaN→NaN・正→1・負→-1・`±0`→そのまま=0/-0、 JS-exact)。 doc の壊れた一文(「NaN instead of NaN」)も修正。
- **`min`/`max` の NaN が非対称**: ilang 実装 `if a<b {a} else {b}` は `min(nan,5)=5`(NaN 第1引数を伝播せず)だが `min(5,nan)=NaN` = **順序依存・非可換**。 doc は「NaN はどちらからでも伝播」とあるので doc 通りに修正([math.il](../../libs/std/math.il)): `if a.isNaN() { a } elif b.isNaN() { b } elif a<b {a} else {b}`。 両側で NaN 伝播・可換。 perf 影響は isNaN 2 回(軽微)。
- **クリーン確認**: clamp/lerp/smoothstep/remap・定義域外 intrinsic(`sqrt(-1)=NaN`・`asin(2)=NaN`・`ln(0)=-Inf`・`pow(0,0)=1`・`cbrt(-8)=-2`・`hypot(3,4)=5`)・丸め(`round` は half-away-from-zero=Rust 準拠)は全て正しい。
- **検証**: fixture `04_modules/math_sign_and_nan.il`。 programs harness JIT+AOT、 workspace 546/546。 **注意**: `math.il` は `include_str!` 埋め込みなので変更後は `cargo build -p ilang` 再ビルド必須(再ビルド前は旧 math.il が走り誤判定する)。

### [解決済み記録] 第 150 弾: break の無い `loop` を関数 tail にすると `body produces ()` (2026-06-15)

`fn f(): i64 { ...; loop { if c { return v } ... } }`(`loop` が tail で break 無し・return のみ)が「body produces ()」で拒否されていた。 第 149 弾の async 修正で表面化したが、 **async/非 async 共通の checker の divergence 制限**(第 144 弾の match-all-return と同系)。

- **原因**([checker/expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs) の `ExprKind::Loop`): loop の型は「`break v` の型、 無ければ `Unit`」。 だが **break が全く無い loop は値に落ちない**(`return` でしか抜けない・または無限ループ)= divergent なのに `Unit` 扱いしていた。 LoopFrame は `Loop(None)` で「break 皆無」を、 `Loop(Some(t))`(bare break は Some(Unit))で「break あり」を区別できる。
- **修正**: `Loop(None)`(break 皆無)のとき `ret_ty`(関数戻り型)を返す。 `return` / 第 144 弾の all-arms-return と同じ近似。 これで `fn f(): i64 { loop { ...return... } }` も `fn f(): i64 { loop {} }`(無限ループ・Rust の `fn f() -> i32 { loop {} }` 同様)も通る。 `break v` あり(`Some(t)`→t)・bare break(`Some(Unit)`→Unit)は不変。
- **検証**: fixture `loop_no_break_diverges.il`(loop tail の return・`return loop{break}`・ネスト loop の return)。 第 149 弾の async firstEven(break 無し loop)もこれで解消。 programs JIT(1709 件)+AOT、 workspace 546/546、 nested_generic 100/100。

### [解決済み記録] 第 149 弾: await を含まない async fn の `return` が Promise にラップされず型エラー (2026-06-15)

`async fn one(): i64 { return 1 }`(await を含まない zero-await async fn で `return` 文を使う)が `expected Promise<i64>, got i64` で拒否されていた。 await があれば(ダミーでも)通り、 tail 式 `{ 1 }` なら通る。

- **原因**([async_desugar/mod.rs](../crates/ilang-parser/src/normalize/async_desugar/mod.rs)): zero-await の trivial lowering は **body の tail 式だけ**を `Promise.resolve` で包み、 body 内の **`return X` 文を包んでいなかった**。 戻り型は `Promise<T>` に wrap されるので、 bare `return X`(=`X`)が `Promise<T>` と不一致。
- **修正**: `wrap_body_in_promise_resolve` を拡張。 (1) 全 `return X` を `return Promise.resolve(X)` に再帰的に書き換え(Block/If/Match/IfLet/loop/let RHS に降りる・**ネスト FnExpr には降りない**)。 (2) tail は「内部 return を包む→ tail が **diverge する**(全パス return/break)なら外側包みしない、 そうでなければ tail 全体を**1回だけ** `Promise.resolve`」。 (3) `loop` tail は break 値と return をリーフで包む(外側包みしない)。 **二重包み回避**(`Promise<Promise<T>>`)と**ジェネリック推論保持**が要点: `if c { Result.ok(..) } else { Result.err(..) }` を arm ごとに包むと join が分断され Result 型引数が推論不能になる(`async_fn_returns_result.il` が一度回帰)ので、 diverge しない tail は**全体 1 回**包む。
- **検証**: fixture `async_zero_await_return.il`(bare return・if/match 分岐 return・stmt 経由 return)。 await あり async・Result 戻り async・closure 内 return 非干渉・loop break 値を確認。 programs harness JIT(1708 件)+ `ILANG_TEST_AOT=1`、 workspace 546/546、 ilang-parser 30/30。 残: 上記 break 無し loop tail の checker 制限(別件)。

### [解決済み記録] 第 148 弾: weak 強制を全消費サイトで完了(field/array/tuple/arg)(2026-06-15) — ユーザー決定

第 145〜147 弾は **let / 再代入 / return** の weak 強制を直したが、 **コンテナ/引数の消費サイト**(field store・array.push・array/tuple リテラル・`T.weak` 引数)は **fresh ソースでまだ UAF** だった(borrowed は strong 保持者頼みで動いていた)。 ユーザー「残り全部直す」方針で完了。

- **検討と却下(coerce 集約)**: ユーザーは当初 coerce 集約を選択。 試したところ **field store / array.push / lower_arg_to は元々 slot 所有のため自前 weak retain しており、 coerce の retain と二重→リーク**(`weak_backref_cascade_release_order` と `leak_fresh_weak_arg` が回帰)。 一方 **array/tuple リテラルは coerce が retain 済みと仮定**して自前 retain しない(コメントは誤りで実際は bare)。 つまりサイトごとに前提が不一致で、 集約は全解放側の対称化が必要な大工事。 `e0edf298` へ戻し、 より安全な **per-site 順序修正**(retain を足すだけ・除去しない)へ切替(ユーザー合意)。
- **修正(per-site 順序修正)**: 各消費サイトで「`StrongToWeak` の weak retain を `release_owned_wrap_source` の**前**に・fresh/borrowed 問わず」。
  - field store([expr.rs](../crates/ilang-mir/src/lower/expr.rs) `store_value_to_field`): coerce 後に retain、 結果を fresh 扱いして汎用 retain と二重回避。
  - 引数([literals.rs](../crates/ilang-mir/src/lower/literals.rs) `lower_arg_to`): Object→Weak で retain、 `last_arg_wrapped` で呼び出し側に所有を伝達。 array.push([calls/array.rs](../crates/ilang-mir/src/lower/calls/array.rs))は `last_arg_wrapped` を見て自前 retain をスキップ。
  - array/tuple リテラル([literals.rs](../crates/ilang-mir/src/lower/literals.rs)): `wrapped && target==Weak` のときだけ retain(Optional は coerce が retain 済みなので除外)。
  - map リテラルは元から正常(coerce 後の release が elem_is_fresh ベースで釣り合っていた)。
- **検証**: fixture `weak_fresh_into_container_and_arg.il`(push/literal/tuple/arg は通常モードで決定的に `BAD` vs `none`、 field は HEAP_GUARD + 個別 churn で担保)。 網羅スイープ: let/再代入/return/field/array.push/literal/tuple/map/arg × borrowed/fresh + 一時レシーバ、 全て none/正値 かつ leak churn 全 leakfree、 HEAP_GUARD 破損無し、 全 weak fixture(20 個超)JIT+guard 緑、 workspace 546/546、 programs JIT+AOT、 nested_generic 100/100。
- **教訓**: weak rc は「strong 解放より前に weak +1・消費/scope 退出で release」を**全サイト**で揃える必要がある。 サイトにより「coerce が retain 済みと仮定」「自前 retain」が混在しており、 集約は両者の対称化が前提。 per-site は retain 追加のみで二重化せず安全だった。

### [解決済み記録] 第 147 弾: weak 強制の残り経路(fresh / join / return)と順序バグを完了 (2026-06-15) — ユーザー決定

第 145/146 弾は **borrowed ソース**の weak 強制しか直しておらず、 **fresh ソース**(`new T()` 直接・if/else / match の join)と **return** 経路がまだ UAF だった。 ユーザー「残り全部直す」方針で完了。

- **見落としの構造**: `is_fresh_object_expr` は `If`/`Match`/`New` を **fresh** と分類する(body_cx.rs:1003/1023)。 第 145 弾の「bare `Weak` を fresh 判定から除外」は `||` の**後段**にあり、 ソースが既に fresh のとき短絡されて束縛 weak retain がスキップされていた。 つまり borrowed(`let w = n`)だけ直り、 fresh(`let w = if…` / `let w = new T()`)は素通り。
- **順序バグ**: weak retain を `release_owned_wrap_source`(strong +1 を落とす)より**後**に出していた。 fresh ソースは他に strong 保持者がいないので、 先に strong を 0 にすると weak 共有が無いまま `__release_object` が box を解放し、 後の weak retain が手遅れになる。 if/else が偶然動いていたのは分岐値がローカルに保持され strong が 0 に落ちなかったため。
- **修正(統一原則)**: `StrongToWeak`(Object→bare `T.weak`)では **ソースの fresh/borrowed を問わず、 strong 解放より前に weak +1 を取得する**。 適用箇所: let([stmt.rs](../crates/ilang-mir/src/lower/stmt.rs))・再代入([expr.rs](../crates/ilang-mir/src/lower/expr.rs) `ExprKind::Assign`)・return([control.rs](../crates/ilang-mir/src/lower/control.rs) `lower_return`)。 いずれも coerce 直後・`release_owned_wrap_source` の前に weak retain を挿入し、 後段の汎用 retain と二重にならないよう調整。 `Object→Optional<Weak>` は coerce 内で weak +1 を取るので対象外。
- **検証**: fixture 2 件追加 — `weak_join_and_fresh_into_weak.il`(if/else・match・`new` を weak へ)・`weak_return_strong_as_weak.il`(fresh return は none・borrowed 生存 return は some)。 どちらもスロット再利用で**通常モードでも決定的**(修正 off で `BAD some v=999`、 on で none)。 stash で実証。 網羅スイープ: let/再代入/return × borrowed/fresh/if-else/match/new + 一時レシーバ、 全て none/正値かつ leak churn 全 leakfree、 HEAP_GUARD 破損無し、 既存 weak fixture 8 件 guard 緑。 workspace 546/546、 programs JIT+AOT、 nested_generic 100/100。
- **教訓**: 第 145 弾を「named 束縛の churn」だけで検証して `fnReturningWeak().method()` の一時リークと fresh ソースの UAF を見逃した。 weak は **束縛・再代入・return・一時消費の各サイト**で「strong を落とす前に weak +1 を取り、 消費・scope 退出で必ず release」を揃える必要がある。

### [解決済み記録] 第 146 弾: weak 再代入の UAF と一時 weak レシーバのリーク (2026-06-15)

第 145 弾(weak 束縛 retain)が **2 つの兄弟問題**を露呈した。 ユーザー要望で weak 強制の全経路を網羅 probe した結果:

- **再代入 `w = strongRef` の UAF**([expr.rs](../crates/ilang-mir/src/lower/expr.rs) の `ExprKind::Assign`): let 束縛と同じ「coercion=fresh」ヒューリスティックを共有しており、 `StrongToWeak` を fresh 扱いして weak retain を省いていた。 第 145 弾と同じく bare `Weak` 標的を除外。
- **一時 weak レシーバのリーク(第 145 弾の回帰)**([calls/mod.rs](../crates/ilang-mir/src/lower/calls/mod.rs)): `mkWeak().get()` のように**関数が返した fresh な weak をメソッドが消費する**とき、 weak メソッド dispatch だけが他(optional/array/string/promise)にある `if obj_is_fresh { Release }` ガードを欠いていた。 第 145 弾で weak 束縛が +1 を持つようになり、 この解放漏れが顕在化(24 bytes/iter リーク)。 weak dispatch にも同ガードを追加。 **テストスイートに `fnReturningWeak().method()` の churn が無かったため第 145 弾で見逃した**反省点。
- **切り分け**: 名前付き `let r = mk(); r.get()` はリークせず(scope 解放で釣り合う)、 一時 `mk().get()` だけ漏れた。 計測(`__retain_weak`/`__release_weak` に env-gated トレース)で「再代入自体はリークせず、 一時レシーバが原因」と確定。 最初に試した素朴な再代入 retain は return と組み合わせるとリークに見えたが、 真因は一時レシーバ側だった。
- **検証**: fixture 2 件追加 — `weak_reassign_keeps_zombie_alive.il`(スロット再利用で通常モードでも `BAD some` vs `ok none` と決定的)・`weak_temp_receiver_release.il`(`mkWeak().get()` を 300 回 churn し `liveAllocBytes` で leak 検出)。 stash で各修正 off にして両 fixture が落ちることを実証。 広範 leak/UAF スイープ(arc 系・named・temp 全て leakfree かつ none)、 既存 weak fixture 8 件が JIT+HEAP_GUARD 緑、 workspace 546/546、 programs JIT+AOT、 nested_generic 100/100。

### [解決済み記録] 第 145 弾: `let w: T.weak = strongRef` が weak 共有を retain せず UAF (2026-06-15) — ユーザー決定(修正案の比較)

`let w: T.weak = obj` の束縛が weak 共有を retain せず、 zombie box(strong rc=0・メモリ有効)が weak の生存中に**解放**され、 `w.get()` が解放済みメモリを読む use-after-free。 通常実行は解放スロットが 0 を読むため偶然 `none`(正)になり露見しないが、 `ILANG_HEAP_GUARD=1` では poison を読んで誤って `some` を返す。

- **発見**: クラス継承/ジェネリクス/ARC を probe 中、 親子循環の後に空クラスへの weak を作ると HEAP_GUARD で `dangling_none` が通常 `true`/guard `false` と食い違った。 二分の結果 **空クラス(フィールド無し)+ 先行する別 alloc/free** で再現(フィールドを1つ足すと解放済みスロットがたまたま rc≤0 を読み露見しない。 UAF 自体は両方で発生)。
- **根本原因**([stmt.rs](../crates/ilang-mir/src/lower/stmt.rs)): `let` 束縛の retain 判定が「coercion が起きたら(`bound != v`)新セルを +1 付きで作る wrap とみなし fresh 扱い→束縛 retain を省く」ヒューリスティックを持つ。 しかし `StrongToWeak`(Object→bare `T.weak`)は +1 を作らない**ポインタ降格**なのに、 ここで誤って fresh 扱いされ束縛 retain が省かれた。 一方 scope 退出の release は出るため、 weak 局所は release 1回・retain 0回で count が underflow し、 強参照解放時に `__release_object` が box を解放(`StrongToWeak` 自体は注釈どおり bare cast が正で、 retain は束縛側の責務)。 weak トレース(`__retain_weak`/`__release_weak` に一時計測)で「束縛 retain 欠落 → 強参照解放で free → 解放後 retain」を確認。
- **修正**: ヒューリスティックから **bare `Weak` 標的を除外**(`bound != v && is_arc_heap && !matches!(bind_ty, Weak)`)。 `bind_ty` が `Weak` になる coercion は `StrongToWeak` だけなので安全・最小。 `Object→Optional<Weak>` は Optional セルを mint するので fresh のまま(別経路、 既に明示 Retain あり)。 これで weak 束縛が retain し、 box は zombie として生存、 `WeakUpgrade` が rc=0 を読み `none`。
- **修正案の比較**(ユーザー要望): (a)runtime の `StrongToWeak` を生成時カウント→却下(Optional 経路が既に明示 Retain しており二重計上→リーク)、 (b)lowering で強参照解放を weak 局所解放より前に並べ替え→却下(順序依存で脆く field/array/return 形を直さない)、 (c)採用案=束縛 retain 判定の Weak 除外(局所的・`StrongToWeak` の bare 意図と整合)。
- **検証**: リーク無し確認(weak を 500 回作る churn で deinit=1000・delta=0、 通常/guard とも 二重解放/破損無し)。 fixture `05_edge_cases/weak_bind_keeps_zombie_alive.il` を追加 — 解放スロットを同型 alloc で踏ませ、 **修正無しなら通常モードでも `BAD some v=999`(UAF)・修正有りで `ok none`** と決定的に分岐(HEAP_GUARD は CI 非実行なので通常モードで落ちる構成にした)。 stash で修正 off にして fixture が落ちることも実証。 workspace 546/546、 programs JIT + AOT、 nested_generic 100/100。

### [解決済み記録] 第 144 弾: 全 arm が return する match を関数末尾に置くと型エラー (2026-06-15)

`if b { return 1 } else { return 2 }` は関数末尾で通るのに、 網羅的な `match` で**全 arm が `return`**(または `break`/`continue`)する同等形は `function ... declared to return i64 but body produces ()` で拒否されていた。 if/else と match の divergence 解析の非対称。

- **原因**([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)): match の結果型計算は diverge する arm を join からスキップする(`?` desugar の err-arm `return Result.err(e)` が fn 戻り型を名乗って ok-arm と衝突するのを防ぐため)。 **全** arm が diverge すると `result_ty` が `None` のままになり、 末尾の `result_ty.unwrap_or(Type::Unit)` で `()` になっていた。 `return X` は `expected`(= fn 戻り型)として型付けされる(expr/mod.rs)ので if/else は両分岐が同じ戻り型で一致するが、 match はその型を捨てていた。
- **修正**: diverge した arm の「見せかけ型」(`bt`)を `diverged_ty` に保持し、 非 diverge arm が一つも無ければ `result_ty.or(diverged_ty)` でそれを採用。 enum match と Optional match の両 path に適用。 これで全 arm return の match は fn 戻り型を名乗り、 if/else と一致。 混在ケース(`?` desugar の ok値/err return)は `result_ty` が Some なので従来どおり不変。 プリミティブ(int/string/bool)match は元々 diverge arm をスキップせず `result_ty` が None にならないため対象外。
- **検証**: fixture `06_enums/match_all_arms_return_tail.il`(enum・Optional の両 path、 全 arm return tail)。 回帰確認: Optional 全 return・ループ内 match 全 break・`?` 演算子の desugar すべて緑。 workspace 546/546、 ilang-types 76/76、 programs JIT + AOT 緑。

### [解決済み記録] 第 143 弾: capability bypass — extern を値に束ねて間接呼び出し (2026-06-15)

第 142 弾の capability enforcement に**抜け穴**があった。 `let f = abs` のように `@extern(C)` 関数(ffi シンク)を値に束ねて `f(-7)` と間接呼び出しすると、 **ffi ゲートを回避**して実行できた(`ffi` 未付与でも成功)。

- **原因**: `let f = abs` は MIR で `make_closure func#N`(N は extern 本体)に lower され、 `f(...)` は `call_indirect` になる。 ゲート判定 `call_cap` は `Inst::Call`(直接呼び出し)だけを見ており、 アドレス materialize(`MakeClosure`/`FuncAddr`)も `call_indirect` も見ていなかった。 直接呼び出し `abs()` は `Inst::Call` なので塞がっていたが、 アドレスを一度値にすると後段の間接呼び出しは call-site ゲートを通らない。
- **AOT の「拒否」は巻き添えだった**: libc を使う再現コードでは AOT がたまたまコンパイルエラーになったが、 計測の結果これは無関係な `libc.doAbort`(C の `abort` を直接呼ぶバインディング関数)を `required_caps` が拾っていただけで、 abs の間接呼び出し自体は AOT も検出していなかった。 自己完結 `@extern(C)` で再現すると JIT・AOT 両方が素通りした。
- **修正**([cap_gate.rs](../crates/ilang-mir/src/passes/cap_gate.rs)): `call_cap` に `Inst::MakeClosure { func }` / `Inst::FuncAddr { func }` を追加し、 **func が extern シンクならアドレス materialize 地点で cap を課す**。 C 関数へのポインタを手に入れた時点が capability 上の「使用」なので、 そこでゲートすれば直接・間接・コールバック渡しの全形を JIT/AOT 一致で塞げる。 非 extern wrapper(`std.fs` ヘルパー等)のアドレスは exempt — その本体が内側の intrinsic 呼び出しでゲートを持つため二重にならない。
- **過剰ゲートしないこと**を確認: `ffi` 付与時は間接呼び出しが実行され `abs(-7)==7` を出力。 qsort/bsearch のコールバックに渡すユーザー ilang 関数(非 extern)はゲートされない。
- **検証**: fixture `11_capabilities/ffi_indirect_{denied,granted}/`(自己完結 extern で libc 巻き添えを排除)を追加。 workspace nextest 546/546、 programs harness JIT + `ILANG_TEST_AOT=1` 緑、 std.fs 間接(第 142 弾の堅牢ケース)・ffi 直接の回帰も確認。
- **ゲートの網羅性を追加検証**: 修正後にゲートが各種 lowering 変換を生き残ることを probe で確認 — 間接呼び出し(make_closure)・仮想ディスパッチ(override メソッド本体)・**async desugar**(await をまたぐ std.fs 呼び出しが生成 poll 関数の第2 resume point に落ちても発火)すべて deny。 数値(負数除算/剰余・i64 wrap・float→int 飽和・int キャスト wrap)・文字列(unicode 長さ/charAt/slice clamp)・Map/Set・クロージャ(per-iteration capture)・optional/Result も併せて probe したが、 いずれも仕様どおりで bug 無し。 async 経路の回帰ガードとして fixture `11_capabilities/file_denied_async/` を追加(複雑な状態機械分割を経てもゲートが残ることを pin)。

### [解決済み記録] 第 142 弾: capability enforcement (ilang.toml) を実装 (2026-06-15) — ユーザー決定

言語の中核設計目標(capability ベースのセキュリティでサプライチェーン攻撃を緩和)の初実装。 `@requires` は従来パースのみ・未 enforce だった。

- **ユーザー決定(設計)**: 「コードに `@requires` を書く」のでなく **`ilang.toml` に `capabilities = ["net","file",…]` と書く方式**。 toml に書けば使える、 書いてなければ **JIT は実行時エラー・AOT はコンパイルエラー**。 デフォルトは **deny**(toml の cap のみ許可)。 語彙は粗粒度 **`net` / `file` / `os` / `ffi`**(file=read+write、 os=env+process)。 FFI(`@extern(C)`)は **`ffi` cap を要求**(抜け穴を塞ぐ)。 toml はエントリ .il から**上方探索**。
- **シンクの特定(実装の核)**: std.fs/os は ilang ソース([libs/std/fs.il](../libs/std/fs.il))で内部は **`@intrinsic("fs.exists")`**(`@extern(C)` でない)。 MIR では `@extern(C)`/`@intrinsic` 宣言への呼び出しは `FuncRef::Local(extern-kind fn)`(または alias で `FuncRef::Extern`)。 必要 cap は **callee の C シンボル**で決定 — `$fs.*`→file・`$os.*`→os・他 `$…`(math/time/regex/test/events)→exempt・非`$` 実 C シンボル→ffi。 由来モジュール方式は **inline がシンクを user 関数へ移すと壊れる**ため、 シンボル方式(inline 非依存)を採用。
- **実装**:
  - **runtime** [caps.rs](../crates/ilang-runtime/src/caps.rs): `GRANTED: AtomicU32`、 `set_granted(bits)`(CLI が Rust から呼ぶ)、 `__cap_require_file/os/ffi/net()`(no-arg、 未許可なら `rt_panic` で exit 1)。
  - **MIR pass** [cap_gate.rs](../crates/ilang-mir/src/passes/cap_gate.rs): `cap_for_symbol` でシンボル→cap。 JIT 用 `insert_gates`(各シンク呼び出し前に `cap_require_*` builtin を挿入 — no-arg なので const ValueId 生成不要)、 AOT 用 `required_caps`(必要 cap 集合)。
  - **codegen**: builtin `$cap.require*` を 4 箇所配線(jit_symbols・PanicAux struct・program_decl の `declare_unit_void`・lower_inst/calls.rs)。
  - **CLI** [manifest.rs](../crates/ilang-cli/src/manifest.rs): entry を `canonicalize` して上方探索で `ilang.toml` を発見し `capabilities` を parse(既存の `[package]`/`[deps]` プロジェクト manifest と同居、 未知 cap はエラー)。 [main.rs](../crates/ilang-cli/src/main.rs) の run_file は `set_granted` + `insert_gates`(JIT、 実行時ゲート)、 build_file は `check_caps`(AOT、 未許可なら codegen 前に**コンパイルエラー**、 バイナリを生成しない)。
- **ハマりどころ(全て解決)**: (1) std.fs は `@extern(C)` でなく `@intrinsic` なので gate が `FuncRef::Extern` だけだと漏れる → extern-kind 宣言への `FuncRef::Local` 呼び出しも対象に。 (2) inline がシンクを user 関数へ移し由来モジュール方式が `ffi` を誤要求 → シンボル方式へ。 (3) `ilang.toml` は**既存のプロジェクト manifest**(`[deps]` でバインディング解決) → 別ファイルでなく `capabilities` キーを同居。 (4) 相対パスの上方探索が repo root に届かない → `canonicalize`。 (5) TOML テーブル(`[deps]`)後に追記すると `deps.capabilities` になる → top-level(先頭)に置く。
- **互換**: deny-by-default のため、 既存の全 `ilang.toml`(テスト fixture・bindings・examples)に `capabilities = ["file","os","ffi","net"]` を top-level 追記(リポジトリ自身のコードは信頼)。 テストハーネスは CLI サブプロセス経由なので enforcement が効く。
- **検証**: 統合テスト [capabilities.rs](../crates/ilang-cli/tests/capabilities.rs) 6 件 — file の deny(JIT 実行時 / AOT ビルド時、 バイナリ非生成)・file grant で実行・manifest 無しで deny・pure はマニフェスト不要・unknown cap エラー。 全儀式: workspace nextest 540/540 + capability 6/6、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 docs syntax.md / syntax_ja.md に capability 節を追記、 `@requires` は意図注釈と明記。
- **残課題(将来)**: `net` のネイティブシンク(現状ネットワークは FFI 経由なので `ffi` でカバー)、 cap の細粒度化(`file.read`/`file.write` 分離)、 ライブラリ単位の grant 委譲、 `@requires` 注釈と manifest の整合チェック。

### [解決済み記録] 第 135 弾: f64 値を f32?(numeric optional)に代入すると SIGSEGV (2026-06-15)

第 134 弾(scalar フィールドへの coerce 漏れ)の同族を「他の格納先」で sweep して発見。

- **症状**: `class C { a: f32?  init() { this.a = 8.5 } }` が SIGSEGV(`made` 印字後の scope-end/teardown でクラッシュ)。 `f32?` フィールドへ f64 値を代入したときだけ。 explicit `8.5f32` リテラル(値が既に f32)や `f64?` フィールドは正常。 第 134 弾の f32 フィールド(非 optional)とは別経路。 配列要素・Map 値・変数再代入・return・tuple の f32 は元から coerce 経路を通り健全で、 **numeric Optional への wrap だけ**が穴だった。
- **原因**: Optional への wrap 判定 — `store_value_to_field` の `needs_optional_wrap`([expr.rs](../crates/ilang-mir/src/lower/expr.rs))と `coerce` の wrap arm([coerce.rs](../crates/ilang-mir/src/lower/coerce.rs))— が `**inner == 値型` の**完全一致**(または obj_shape の subtype wrap、 weak downgrade)しか扱わない。 inner=F32・値=F64 はどれにも当たらず、 wrap も demote もされずに **raw f64 が f32? スロットに格納**される。 Optional セルの read / release で f64/f32 の幅不一致がクラッシュを起こす。
- **修正**: 両所の wrap 条件に `inner.is_numeric() && 値.is_numeric() && **inner != 値型`(numeric_wrap)を追加。 coerce 側では box する前に値を inner 型へ `self.coerce`(f64→f32 は fdemote、 i32→i64 は拡大)してから `NewOptional`。 store_value_to_field 側はこの条件で `needs_optional_wrap` を真にして coerce 経路へ流す。 obj_shape / weak / 同型の既存ケースは不変。
- **新規/既存**: numeric optional 導入以来の既存バグ。 既存の optional fixture は同型(`f64? = f64`)か explicit f32 リテラルか object 系で、 「異なる numeric を optional に wrap」が未踏だった。 第 134 弾と同じく「explicit でない float が暗黙 coerce 経路を通る」組合せの穴。
- **検証**: f32? フィールド(init の f64 直代入・`some(1.25)` 代入・`none` 代入・読み戻し)・`let f32? = 8.5`・f32? 引数・f32? return・`i32 → i64?` 拡大・f32? の array リテラル・`Map<string, f32?>` を JIT/AOT + `ILANG_HEAP_GUARD=1` クリーンで確認。 ilang-mir(coerce + lowering)を触ったので全儀式実施: workspace nextest 540/540、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 fixture: `05_edge_cases/numeric_optional_wrap_coerce.il`。

### [解決済み記録] 第 134 弾: f64 値を f32 フィールドに直接代入すると 0.0 (2026-06-15)

第 132 弾の「値が文脈ごとに別フォーマッタを通る」着眼を float の表示に当てる過程で、 表示でなく **store/load** の実バグを発見。

- **症状**: `class C { x: f32  init() { this.x = 1.5 } }` で `c.x` が `0.0`(1.5 でない)。 代入値に関係なく f32 フィールドは常に 0.0(`this.x = 2.5` でも post-construction `c.x = 4.5` でも)。 同じ値でも 直接変数・tuple・optional・array の f32 は正常で、 **object フィールドの f32 だけ**壊れる(f64 フィールドは正常、 i64/i32 も正常)。
- **原因**: ARC object フィールドは 8 バイトスロット([objects.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/objects.rs) の store は `extend_to_i64`、 load は `reduce_from_i64`。 両ヘルパーは f32 を bitcast i32↔uextend で正しく往復する)。 問題は MIR 側: `store_value_to_field`([expr.rs](../crates/ilang-mir/src/lower/expr.rs))が Optional-wrap / Strong→Weak の特殊ケース以外の scalar fall-through で **値をフィールド型へ coerce せず** `vv0` をそのまま store。 f64 リテラル `1.5`(`0x3FF8000000000000`)が f32 フィールドに来ても fdemote されず、 store は f64 を 8 バイトで書き、 load は f32 として**下位 32bit**(`0x00000000` = 0.0)を読む。 1.5/2.5 等は下位ワードが 0 なので 0.0 になる。 f64 フィールドは 8 バイトで一致、 i64/i32 は extend で低位が正しいので無事だった。
- **修正**: `store_value_to_field` の fall-through に分岐を追加 — `vty != *fty && vty.is_numeric() && fty.is_numeric()` のとき `self.coerce(vv0, &vty, fty)`(f64→f32 は fdemote、 他の numeric も正しく変換)。 heap/object は numeric でないので不変、 同型 scalar(f32→f32 等)も `vty != fty` で除外。
- **新規/既存**: f32 フィールド導入以来の既存バグ。 既存の f32 フィールド fixture(`class_derive_float_field.il`・`by_value_float_field_error.il` 等)は全て **explicit `1.0f32` リテラル**や `init(x: f32) { this.x = x }`(引数 coerce で x が既に f32)経由で値が f32 だったため発火せず、 「**f64 値(リテラル/式)を f32 フィールドへ直代入**」という組合せが未踏だった。 539+ 緑を維持していたのはこのため。
- **検証**: f32 フィールドへの f64 リテラル直代入(init / post-construction)・f64 式からの代入・f32 フィールドの arithmetic(`len2 = x*x + y*y` が正)・object 配列内の f32 フィールド・fpromote(f32 値→f64 フィールド)・int フィールド非回帰(i32 from i64 const `2000000000`)を JIT/AOT で確認。 既存 f32 derive fixture も pass。 ilang-mir(lowering)を触ったので全儀式実施: workspace nextest 540/540、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 fixture: `02_classes/f32_field_from_f64_value.il`。 syntax docs は f32 フィールドの内部表現の話なので変更なし。

### [解決済み記録] 第 132 弾: u64 の除算/剰余/シフト(リテラル)と表示が符号付き (2026-06-15)

値等価系統が一巡したので離れた面(数値 coercion)を probe して発見。 2 つの独立した既存バグ。

- **症状 1(算術)**: `big / 2`(big: u64, big ≥ 2^63)が負の値になる。 `big % 7`・`big >> 1` も符号付き演算。 一方 `big / two`(`two: u64`、 明示型の divisor)は正しい。 つまり **divisor が裸の整数リテラルのとき**だけ符号付き。 原因: リテラル `2` は MIR に i64 として届き u64 と同幅。 `unify_numeric`([coerce.rs](../crates/ilang-mir/src/lower/coerce.rs))の cross-sign 整数の tie-break が「同幅なら符号付き優先」(FFI の size_t↔i64 を意図)なので i64 を選び、 `IDivS`/`IRemS`/`IShrS`(算術シフト)を使う。 値 ≥ 2^63 で誤答。 checker はリテラルを u64 に adopt するのに MIR がそれを反映していなかった。
- **症状 2(表示)**: u64 の値 ≥ 2^63 が `toString`/`console.log`/配列表示/`${}` で符号付き 10 進(`18e18` → `-446744073709551616`)。 Map の値だけは `PK_I64_UNS` 経由で正しかった(非対称が手掛かり)。 原因: 各表示経路が i64 ビットを `__int_to_string`/`__print_int`/`__fmt_int`(符号付き)で出力。 narrow unsigned(u8/u16/u32)は i64 へゼロ拡張で正なので問題は u64 のみ。 比較は元から unsigned(`IcmpU`)で正常。
- **修正 1**: `lower_binary`([ops.rs](../crates/ilang-mir/src/lower/ops.rs))で `unify_numeric` の前に、 一方が `ExprKind::Int` リテラルで両者が同幅 cross-sign 整数のとき、 リテラルを相手の整数型へ retype(ビット不変なので値変換不要)。 これで `big / 2` は両 u64 → `unify_numeric` 早期 return → `IDivU`。 **変数どうし**の cross-sign は両方リテラルでないので不変(FFI 非回帰)。
- **修正 2**: runtime に unsigned 版 `__uint_to_string`([strings.rs](../crates/ilang-runtime/src/strings.rs))・`__print_uint`([print.rs](../crates/ilang-runtime/src/print.rs))・`__fmt_uint`([fmt.rs](../crates/ilang-runtime/src/fmt.rs))を追加(`(n as u64).to_string()`)。 toString([scalar.rs](../crates/ilang-mir/src/lower/calls/scalar.rs))・console.log/配列([print_emit.rs](../crates/ilang-mir-codegen/src/compile/print_emit.rs))・`${}`([fmt_emit.rs](../crates/ilang-mir-codegen/src/compile/fmt_emit.rs))の整数アームで `MirTy::U64` のみ unsigned フォーマッタへ。 builtin(`$string.fromUint`/`$print.uint`/`$fmt.uint`)を各 3 箇所(jit_symbols・program_decl の StrIds/PrintIds/FmtIds・lower_inst/print_emit/fmt_emit)に配線。
- **新規/既存**: u64 導入以来の既存バグ。 第 93/94 弾は u64 の符号なし比較/除算/シフトを sweep 済みと記録していたが、 テスト値が < 2^63 で signed==unsigned が一致しており、 (a) divisor が**リテラル**のケースと (b) 値 ≥ 2^63 の**表示**を見逃していた。 兄弟経路(Map 値表示)が正しかったことが症状 2 の手掛かり。
- **検証**: `big/2`==9e18・`big%7`==4・`2^63>>1`==2^62(論理)・明示型 divisor と一致・toString/`${}`/console.log/配列が全て unsigned 表示・signed i64 の `/`(-10/2=-5)・`%`(-10%3=-1)・算術シフト(-8>>1=-4)非回帰・比較 unsigned 非回帰を JIT/AOT で確認。 ilang-mir(lowering)+ runtime + codegen を触ったので全儀式実施: workspace nextest 540/540、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 docs syntax.md / syntax_ja.md の toString 節に u64 unsigned 表示とリテラル adopt を追記。 fixture: `01_basics/u64_signedness.il`。

### [解決済み記録] 第 130 弾: payload enum を直接 Set 要素 / Map キーに (2026-06-15) — ユーザー決定

第 129 弾の直接の兄弟経路。 enum に equals/hashCode を付けた帰結。

- **発端の不整合**: 第 129 弾で enum に構造的 `equals`/`hashCode` を付けたのに、 payload enum を **直接** `Set<Shape>` 要素 / `Map<Shape, V>` キーにすると「set element type Shape — class Shape must declare equals/hashCode」で拒否される。 一方 payload enum を `@derive(Eq, Hash)` キークラスの**フィールド**にすればキーになれる(第 129 弾)。 原因: checker の `enum_is_value_keyable`([sigs.rs](../crates/ilang-types/src/checker/sigs.rs))が unit-variant / `@flags` enum のみ許可(payload enum は「言語に enum の eq/hash 協定が無い」ため拒否 — 第 129 弾でその協定を追加済みだったのに未追従)。
- **ユーザー判断**: 「(1) 今実装」「(2) 制限として記録」を提示し **(1) 実装**を選択。
- **実装(中規模、 メモリ安全性に注意)**:
  - **runtime のキー ARC を種別対応に**: object-keyed store(`ObjectStore`@[sets.rs](../crates/ilang-runtime/src/sets.rs) / `ObjectMapStore`@[maps.rs](../crates/ilang-runtime/src/maps.rs))に `key_kind: i64` を追加。 全キー retain/release(insert/remove/clear の内部 3 + values/forEach/keys/entries/set 演算 iteration の外部 ~12)を `__retain_object`/`__release_object` 固定から `retain_field_by_kind`/`release_field_by_kind`(cascade の種別ディスパッチ)へ。 enum の rc は object と別オフセットなので、 種別固定だとメモリ破壊する。 内部箇所は `self.key_kind`(disjoint フィールド)を直接渡してバケット借用との衝突を回避。
  - **専用 constructor**: `__set_new_enum()` / `__map_new_enum()` が `__enum_structural_eq`/`__enum_structural_hash` と `key_kind = KIND_ENUM` を内部配線(enum helper は runtime 関数なので fnptr 受け渡しが不要 — class キーの `FuncAddr` 経路を回避)。 print kind は `PK_ENUM`。
  - **print**: `PK_ENUM`([kind.rs](../crates/ilang-runtime/src/kind.rs))を新設、 print_dispatch が eid を `ptr-8` から読んで `format_enum_into` で整形。 iteration の elem_kind 導出と set 演算結果セットの print kind 継承も `PK_ENUM`/`KIND_ENUM` 対応。
  - **checker**: `enum_is_value_keyable` を全 enum 許可に。 **lowering**([expr.rs](../crates/ilang-mir/src/lower/expr.rs)): `enum_has_payload(eid)` で payload enum を判定し、 Set/Map を `set_new_enum`/`map_new_enum` builtin へ振り分け。 unit/@flags enum は従来どおり i64-tag store(`NewMap`/`set_new`)に fall-through。 builtin を 4 箇所配線(jit_symbols・mod.rs の `MapIds`/`SetIds` に `new_enum`・program_decl の `declare_returns_i64`・calls.rs)。
- **検証**: Set dedup(`circle(5)`×2 → 1、 `circle(9)` 別、 `named("hi")`×2 → 1、 unit `dot`×2 → 1)・`has`・`delete`・Map キー上書き(`m[circle(1)]=10` then `=20` → 20)・`size`・forEach/keys iteration での enum 復元・**heap payload を持つ enum キーの churn ARC**(`Holder.box(new Box(i))` を 100 回、 set drop で Box が cascade 解放 → delta=0・deinit=100・`ILANG_HEAP_GUARD=1` クリーン)・print(`Set { Shape::circle(9), Shape::dot, Shape::named(hi) }`)を確認。 オブジェクト payload は参照比較(別インスタンスは別キー)。 既存 Set/Map fixture(class キー・unit enum キー)全 pass。 旧 expect-error fixture `set_of_payload_enum_error.il`(payload enum Set 拒否を主張)を削除(挙動を意図的に変えたため)。 runtime + checker + lowering + codegen を触ったので全儀式実施: workspace nextest 540/540、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 docs syntax.md / syntax_ja.md の Set 要素 / Map キー節を更新。 fixture: `03_collections/set_map_payload_enum_key.il`。

### [解決済み記録] 第 129 弾: enum フィールドを @derive(Eq, Hash) で完全支援 (2026-06-15) — ユーザー決定

第 127 弾の続きで `@derive(Eq, Hash)` のフィールド型を sweep して発見。

- **発端のバグ**: enum フィールドを持つクラスに `@derive(Eq, Hash)`(または `Eq` 単独)を付けると、 「undefined class "Color"」という誤診断が出る。 array/optional フィールドは正しく「field has type … which is not supported by auto-derived hashCode」と言うのに、 enum だけ別の壊れ方。 原因: 合成 equals `field_eq_expr`([derive.rs](../crates/ilang-parser/src/loader/derive.rs))が `Type::Object` フィールドに `this.f.equals(other.f)` を生成するが、 loader 段階では enum も class も `Type::Object(name)` で区別できず、 enum は `.equals()` を持たないため後段の checker が `Color` を class として解決しようとして失敗。
- **ユーザー判断**: 「(1) Eq は構造的支援・Hash は明確なエラー」「(2) Eq も Hash も完全支援」「(3) 両方とも明確なエラー」を提示し、 **(2) 完全支援**(enum フィールドのクラスがそのまま Set/Map キーになれる)を選択。
- **実装方針**: enum 値に組込みメソッド `.equals(other): bool` / `.hashCode(): i64` を追加する方式。 これで derive の合成コード(`this.f.equals(...)` / `this.f.hashCode()`)が enum フィールドでも無変更で通り、 standalone でも enum メソッドが使える。
  - **runtime**: `__enum_structural_hash(a)`([enums.rs](../crates/ilang-runtime/src/enums.rs))を新設 — enum id + 判別子タグ + 各 payload slot の構造的ハッシュを `h*31+e` で畳み込む。 汎用ディスパッチャ `value_structural_hash(v, kind)` と tuple/array/optional のハッシュを [equality.rs](../crates/ilang-runtime/src/equality.rs) に追加(第 127 弾の `value_structural_eq` と対称 — 数値=値・string=`__str_hash_code`・enum/tuple/array/optional=構造的再帰・参照型=ポインタ値)。 `__enum_structural_eq` が構造的に等しい値は `__enum_structural_hash` も等しい(equals/hashCode 契約)。
  - **checker**: [calls.rs](../crates/ilang-types/src/checker/expr/calls.rs) の `check_method_call` で、 受信側が enum(`Type::Object(name)` かつ `self.enums` に登録)のとき `hashCode`→`I64`・`equals(other)`→`Bool`(引数は同 enum 型)を認識。 **注意**: checker では enum 値は `Type::Enum` でなく `Type::Object(name)`(`self.enums` で判別)で表現される — 最初 `Type::Enum` で書いて一致せず「undefined class」が直らなかった。
  - **lowering**: [scalar.rs](../crates/ilang-mir/src/lower/calls/scalar.rs) の `try_lower_scalar_method` で `MirTy::Enum` 受信側の `hashCode`→`enum_structural_hash`・`equals`→`enum_structural_eq` builtin 呼び出し(MIR では enum は `MirTy::Enum`)。 fresh 受信側は呼び出し元([mod.rs](../crates/ilang-mir/src/lower/calls/mod.rs))の `obj_is_fresh && is_arc_heap` で解放(プリミティブは heap でないので無害)、 fresh equals 引数は scalar.rs 内で解放。 builtin `enum_structural_hash`(`$enum.structuralHash`、 i64→i64)を 4 箇所配線(jit_symbols・mod.rs struct・program_decl の `declare_unary_i64`・calls.rs)。
- **検証**: unit/数値 payload/文字列 payload/複数 slot payload(2 つ目の slot 違いを区別)の enum フィールドで Set dedup(size 厳密)・`has`・Map キー上書き・hash と eq の一貫(等価値は等価ハッシュ)・standalone `Tag.num(5).equals(...)` / `.hashCode()`・fresh key の churn ARC(`s.has(new Key(...))` 100 回で delta=0・`ILANG_HEAP_GUARD=1` クリーン)を確認。 array/optional/tuple フィールドは従来通り clean に拒否(診断の「Supported field types」に「enums」を追記)。 既存 derive fixture(string/float/nested/mixed/Set/Map)全 pass。 runtime + checker + lowering + codegen を触ったので全儀式実施: workspace nextest 540/540、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 docs syntax.md / syntax_ja.md に enum の equals/hashCode を追記。 fixture: `02_classes/derive_enum_field.il`。

### [解決済み記録] 第 127 弾: tuple/動的配列/optional に構造的 `==` を導入 (2026-06-15) — ユーザー決定

第 126 弾の続きで「`==` を使う他の機構」を sweep し、 配列検索の不整合から仕様判断に至った。

- **発端の不整合**: `==` が未定義の型に対する配列検索が、 checker を通り常に無意味な結果を返す。 `(1,2) == (1,2)` は「cannot apply binary op」で**拒否**されるのに、 `[(1,2)].includes((1,2))` は**コンパイルが通り false**(fresh tuple は構築毎に別ポインタ、 runtime の `cell_matches` がポインタ比較に落ちる)。 動的配列要素・optional 要素も同様(常に false/-1/no-op)。 object 要素は参照比較で一貫(`==` も参照)、 string/enum 要素は構造的で一貫していた。
- **ユーザー判断**: 選択肢を「(1) checker で拒否」「(2) 構造的 `==` を実装」「(3) 参照比較で許可」の 3 つを ilang コードの挙動差とともに提示。 **(2) 構造的 `==` を実装**(enum/string と同じ値等価を全 heap 値型へ広げる)を選択。
- **実装**:
  - **runtime**: 共通ディスパッチャ `value_structural_eq(a, b, kind)` と extern `__tuple_structural_eq`/`__array_structural_eq`/`__optional_structural_eq` を新モジュール [equality.rs](../crates/ilang-runtime/src/equality.rs) に新設。 tuple は packed ワード(arity + 4bit×12 の要素 kind)、 array はヘッダ(len/data/elem kind/stride)、 optional はセル(value/inner kind、 none=null)を読み、 kind ごとに分岐 — `KIND_NONE`=ビット、 `KIND_STR`=`__str_eq`、 `KIND_ENUM`=`__enum_structural_eq`、 `KIND_TUPLE/ARRAY/OPTIONAL`=各構造的比較(相互再帰)、 その他 heap(object/map/set/closure/weak/promise)=ポインタ。 `load_packed` を `pub(crate)` 化。
  - **統一**: `cell_matches`([arrays.rs](../crates/ilang-runtime/src/arrays.rs))の enum 専用分岐を tuple/array/optional 込みで `value_structural_eq` 呼び出しに拡張。 `__enum_structural_eq` の payload ループ([enums.rs](../crates/ilang-runtime/src/enums.rs))も同ディスパッチャへ — enum の tuple/array/optional payload も構造的になる(副次改善)。
  - **演算子**: checker([expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs))の Eq/Ne で、 同型 tuple・動的 array(`fixed: None`、 elem 一致)・optional(inner 一致 or どちらか Any = `x == none` 対応)を `Bool` として許可。 lowering([ops.rs](../crates/ilang-mir/src/lower/ops.rs))が MirTy で per-type builtin(`tuple/array/optional_structural_eq`)へ振り分け、 Ne は結果を否定、 fresh オペランドを比較後 Release(借用のみ)。 builtin を enum と同様 4 箇所配線(jit_symbols・mod.rs の panic_aux struct・program_decl の `declare_binary_i64`・calls.rs の Symbol→FuncId)。
  - **対象外**: 固定長配列(`fixed: Some(n)`)は別レイアウトのため演算子・検索とも非対象。 順序比較(`<` `<=` `>` `>=`)は型エラーのまま(値型に自然順序なし)。
- **検証**: tuple(値一致・不一致・`!=`・文字列スロット・入れ子再帰・heap スロットは参照比較で `(p,1)==(q,1)`=false)、 動的 array(一致・長さ違い・文字列要素)、 optional(some 一致/不一致・some vs none・`none==none`)、 配列 `includes`/`indexOf`/`remove`(tuple/array/optional 要素の構造的一致・remove は最初の一致のみ)、 fresh オペランド/needle の churn ARC(`==` 比較も配列検索も delta=0・`ILANG_HEAP_GUARD=1` クリーン・過剰解放なし)を確認。 enum の tuple payload 構造的比較(`E.pt((1,2)) == E.pt((1,2))`=true)も確認。 旧ユニットテスト `array_equality_rejected`(配列 == 拒否を主張)を `dynamic_array_equality_is_structural` + `tuple_and_optional_equality_is_structural`(構造的・Bool)へ更新。 runtime + checker + lowering + codegen を触ったので全儀式実施: workspace nextest 540/540、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 docs syntax.md / syntax_ja.md に構造的 == の規則を追記。 fixture: `05_edge_cases/structural_eq_tuple_array_optional.il`・`03_collections/array_search_structural_containers.il`。

### [解決済み記録] 第 126 弾: 配列 indexOf/includes/remove が payload enum を構造的に比較しない (2026-06-15)

第 105 弾(enum `==` の構造的比較)の**兄弟経路**を「`==` を使う他の機構」から探して発見。

- **症状**: payload を持つ enum を要素とする配列で `includes`/`indexOf`/`remove` が構造的に一致せず、 在る要素を見つけられない。 `[Shape.circle(5), Shape.dot, Shape.named("hi"), Shape.circle(9)].includes(Shape.circle(5))` が **false**、 `indexOf(Shape.circle(9))` が **-1**。 `indexOf(Shape.dot)`(unit variant)だけは **1** で正しい。 直接 `==` は第 105 弾以降 `Shape.circle(5) == Shape.circle(5)` = true(構造的)なので、 配列検索との不整合。
- **原因**: runtime の要素比較 `cell_matches`([arrays.rs](../crates/ilang-runtime/src/arrays.rs))が `stored == needle`(生のビット/ポインタ等価)と `KIND_STR`(`cstr_bytes` で内容比較)の 2 分岐しか持たず、 **KIND_ENUM の分岐が無い**。 そのため enum 要素はポインタ等価に落ちる。 unit variant は `__enum_unit_get` で intern された singleton なので構築の度に同一ポインタ → 偶然一致。 payload variant(`circle(n)`/`named(s)`)は構築毎に新規ヒープセルを box するため、 needle と stored のポインタが必ず異なり一致しない。
- **修正**: `cell_matches` に `elem_tag == KIND_ENUM` の分岐を追加し、 第 105 弾で導入した `__enum_structural_eq(stored, needle)`([enums.rs](../crates/ilang-runtime/src/enums.rs)、 判別子一致 → payload kind ごとにビット/`__str_eq`/再帰 enum で比較)を呼ぶ。 `indexOf`/`includes`/`remove` は全て `cell_matches` を通る(`includes` は `indexOf >= 0`、 `remove` も同 helper)ので 1 箇所で 3 演算が是正。
- **新規/既存**: 配列検索は第 105 弾以前から enum をポインタ比較しており payload enum を見つけられなかった(その頃は直接 `==` も判別子のみで別の壊れ方)。 第 105 弾が `==` を構造的にしたことで「`==` は構造的・配列検索はポインタ」という不整合が顕在化した。 今回 `==` 側の構造的意味論に揃えた。 enum は `@derive(Eq, Hash)` 不可・methods で equals/hashCode も宣言不可のため Set 要素 / Map キーには非対応(checker が clean に拒否、 ハッシュ経路の同型の穴は存在しない)。
- **検証**: 単数 payload(`circle: (i64)`)・文字列 payload(`named: (string)`)・複数 slot payload(`pair: (i64, string)`、 2 つ目の slot 違いを区別)・unit variant・否定(不在は -1/false)・`remove` が最初の構造的一致のみ削除し残りをシフト・fresh enum needle の churn ARC(`includes`/`indexOf` が needle を borrow するだけで delta=0)を確認。 runtime(arrays.rs)のみ変更のため workspace nextest 539/539 + AOT 全 program fixtures PASS を実施(lowering 不変のため nested_generic 並列儀式は省略)。 fixture: `03_collections/array_search_enum_structural.il`。 docs syntax は enum `==` の構造的意味論を第 105 弾で記載済みで、 配列検索がそれに従っただけのため変更なし。

### [解決済み記録] 第 124 弾: `is`/`as?` が第一基底以外の interface を認識しない (2026-06-15)

第 119–123 弾の interface dispatch を一巡したあと、 dispatch でなく**型テスト**経路(`is`/`as?`)を probe して発見。

- **症状**: クラスが第一基底以外で実装する interface に対する `is`/`as?` が誤って false/none。 (1) `class Impl: C`(`interface C: B`, `B: A`)で `im is C`=true なのに `im is B`/`im is A`=**false**(interface 継承で得た祖先)。 (2) `class Impl: A, X` で `im is A`=true なのに `im is X`=**false**(`interfaces` リストの 2 つ目)。 対応する `as?` も `none` を返す。 checker は `a is B` 等をコンパイルエラーにせず通す(到達可能と判断)のに runtime が false を返す不整合。 dispatch(`a.bar()` 経由のメソッド呼び出し)は別経路(vtable)なので正しく動いており、 型テストだけが壊れていた。
- **原因**: runtime 型テスト `emit_is_subclass`([mod.rs](../crates/ilang-mir-codegen/src/compile/mod.rs))が accept 集合(= target の全 is-a 子孫 class-id)を **`ClassLayout.parent` フィールドのみ**を辿る固定点スキャンで構築していた。 クラスは**最初の** interface 基底を `parent` に記録する(`class Impl: C` なら Impl.parent=C)が、 (a) interface 継承 `interface C: B` は interface shell の `parent` でなく lowering 時の `iface_parents` map に、 (b) 2 つ目以降の実装 interface は `interfaces` リストに格納され、 どちらも `parent` に乗らない。 結果スキャンは最初の 1 つの interface しか accept に入れられなかった。
- **修正**: (1) `ClassLayout` に推移的 interface 適合集合 `implements: Vec<ClassId>` を追加([program.rs](../crates/ilang-mir/src/program.rs))。 (2) class lowering で、 既に祖先 interface まで展開済みの `declared_ifaces`(第 120 弾の vtable 登録と同じ push_iface_chain 由来 — 自クラスの全宣言 interface + その `interface B: A` 祖先)を class-id へ変換して `layout.implements` に格納([class.rs](../crates/ilang-mir/src/lower/decl/class.rs))。 (3) `emit_is_subclass` の固定点スキャンを、 `c.parent ∈ accept`(class 継承 + 第一 interface 基底)に加えて `c.implements ∩ accept ≠ ∅`(追加 + 継承 interface)でも `c` を accept に入れるよう拡張。 親クラス経由で継承した interface は、 親クラスが自身の `implements` を持ち `parent` エッジで合流するため自動的に被覆(`class Derived: Base`, `Base: A` で `derived is A`=true)。
- **新規/既存**: 複数 interface 実装(`class C: A, B`)は `multiple_interface_impl.il`(2026-06-07)から存在し interface 継承(7ef5d067、 2026-06-15)より前 — `interfaces` リスト分の `is`/`as?` ギャップは**既存バグ**。 interface 継承で得る祖先 interface 分は新機能の実装漏れ。 いずれも dispatch(vtable)は動いていたため型テストの穴だけが残っていた。
- **検証**: 推移 interface 継承(Impl が C/B/A 全て true)・複数 interface(A/X 両 true)・`as?` の some 化と継承/追加メソッドの dispatch(`b.bar()`=2, `x.qux()`=4)・否定方向(`OnlyA is B/C/X`=false、 未実装 `Unrel`=false で過剰受理なし)・兄弟クラス(`Dog is Cat`=false)・クラス階層(`Dog is Animal`=true)の非回帰を確認。 ilang-mir(program/lowering)+ ilang-mir-codegen を触ったので全儀式実施: workspace nextest 539/539、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 fixture: `09_subtyping/interface_is_as_additional_and_inherited.il`。 docs syntax は `is`/`as?` の意味論(全実装 interface に対し真)が元の仕様どおりで実装が追従しただけのため変更なし。

### [解決済み記録] 第 120 弾: interface 実装クラスのサブクラスを親クラス型で呼ぶと SIGSEGV (2026-06-15)

interface 継承の継ぎ目を「interface 実装クラス × サブクラス × 親クラス型レシーバ × override」で probe して発見。

- **症状**: interface を実装するクラス `Base` のサブクラス `Derived` のインスタンスを **親クラス型** `Base` の変数で受け、 interface メソッドを呼ぶと SIGSEGV(exit 139)。 `interface A { foo(): i64 }; class Base: A { foo(){1} }; class Derived: Base {}; let b: Base = new Derived(); b.foo()` がクラッシュ。 一方 (1) interface 抜き(`class Base`/`Derived` のみ)、 (2) サブクラス抜き(`Base` インスタンスを `Base` 型で)、 (3) **interface 型レシーバ**(`let a: A = new Derived(); a.foo()`)、 (4) 非 interface メソッド(`b.other()`)、 (5) concrete 型レシーバ(`let d = new Derived(); d.foo()`)は全て正常。 override の有無は無関係(空サブクラスでもクラッシュ)。
- **原因**: interface を実装するクラスは各 interface メソッドを **2 つの vtable スロット**に登録する — クラスメソッドスロット(例 `0`)と interface の高位スロット(`IFACE_SLOT_BASE = 1<<20 = 1048576`)。 MIR dump で `Base` は foo を slot 0 と 1048576 の **両方**に持つのに、 `Derived` は **1048576 のみ**でクラススロット 0 を欠いていた。 サブクラスのスロット割り当て([class.rs](../crates/ilang-mir/src/lower/decl/class.rs))が親の `methods` から `parent_slots: HashMap<Symbol, VTableSlot>`(**name→単一 slot**)を作る際、 親の foo の 2 エントリが衝突し、 後から push される高位 interface スロット(1048576)が `collect` で勝ってクラススロット 0 を潰していた。 結果、 サブクラスの継承メソッドが「クラススロット = 1048576」を持ち、 クラス vtable のスロット 0 が空のまま。 親クラス型からの呼び出し(`lower_class_method_call` の VirtCall)はクラススロットを引くため、 空スロットの未初期化関数ポインタを deref してクラッシュ。 interface 型レシーバ(`lower_iface_dispatch`)は高位スロットを使うため偶然動き、 バグを隠していた。
- **修正**: (1) `parent_slots` を **`HashMap<Symbol, Vec<VTableSlot>>`** にして親の全スロット(クラス + interface)を保持。 (2) 各メソッドには `< IFACE_SLOT_BASE` のクラススロットを割り当て(無ければ新規クラススロット)。 (3) 親から継承した interface スロットは、 ループ後に **(override 後の)実装 func を指す MethodDecl** として再登録 — interface 型レシーバでのサブクラス dispatch が引き続き当たる。 (4) 新メソッドの `next_slot` を **クラス範囲のスロットのみ**から計算(従来は max に interface スロットが混じり 1048577 から採番する潜在不具合があった)。 (5) 親継承 + 自身再宣言で同一 interface スロットに二重到達する場合に備え、 最後に `(name, slot)` で dedup。 これで `Derived` が `Base` と同じく foo を slot 0 と 1048576 の両方に持つ。
- **新規/既存**: `parent_slots` の name→単一 slot 衝突と interface スロットの二重登録はいずれも 32fcb938(モジュール分割リファクタ、 7ef5d067 の祖先)以前から存在し、 interface 継承コミットより前からの **既存バグ**。 「interface 実装クラスのサブクラスを親クラス型で受けて interface メソッドを呼ぶ」組合せが未 probe だった(既存 fixture は interface 型レシーバか単層クラスのみ)。
- **検証**: 単純な単一 interface・interface 継承(B:A)・多段継承(GrandChild の override を全レシーバ型で)・override(親クラス型/ interface 型双方で override に命中)・サブクラスへの新メソッド追加(next_slot 衝突なし)・親クラス型配列の混在サブクラス動的ディスパッチ(1+10+100=111)・heap 返却 interface メソッドの ARC(churn delta=0、 deinit 厳密、 `ILANG_HEAP_GUARD=1` クリーン、 過剰解放なし)を確認。 ilang-mir(lowering)を触ったので全儀式実施: workspace nextest 539/539、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 fixture: `09_subtyping/interface_method_base_typed_receiver.il`。 docs syntax は vtable レイアウトの内部詳細のため変更なし。

### [解決済み記録] 第 119 弾: 仮想ディスパッチ呼び出しの fresh heap 引数がリーク (2026-06-15)

直近の interface 継承(7ef5d067)の継ぎ目を「inherited メソッド × fresh heap 引数」で probe して発見。

- **症状**: interface 経由でメソッドを呼ぶとき fresh heap 値を引数に渡すと、 その引数が呼び出し毎に 1 個リークする。 `iface.method(new Box())`(receiver が interface 型 → VirtCall)で deinit が 1 個不足(2 個死ぬはずが 1 個)、 churn で `delta=2400`(100 回 × 24 バイト)と線形増加。 同型の `obj.fnField(new Box())`(closure フィールド呼び出し)も同症状(deinit=0, delta=24)。 concrete 受信側の直接呼び出し(`let a = new Impl(); a.method(new Box())`)は健全(deinit=1)。
- **切り分け**: 戻り値が heap の inherited メソッド(`make(): Box`)は健全 = 戻り値経路は無関係、 **引数経路**のリーク。 free 関数呼び出し・closure 変数呼び出しは健全で、 leak するのは interface dispatch と fn-field 呼び出しの 2 経路のみ。 interface 継承コミット以前から `lower_iface_dispatch` の引数 lower ループには fresh-arg 解放が無く(`git log -L`で確認)、 **既存バグ**。 継承で interface dispatch が踏まれやすくなり顕在化した。
- **原因**: ARC 規約では引数は借用で、 fresh transient(+1)の解放は呼び出し側の責任。 直接パス `lower_class_method_call`([object.rs](../crates/ilang-mir/src/lower/calls/object.rs))は `fresh_obj_args` に fresh 引数を集め呼び出し後に Release していたが、 `lower_iface_dispatch`(VirtCall)と `lower_fn_field_call`(CallIndirect)は引数を `lower_arg_to` で lower するだけで解放経路が抜けていた。
- **修正**: 両関数に直接パスと同じ fresh-arg 解放ロジックを移植 — 各引数で `arg_is_fresh = is_fresh_object_expr(a)`、 `lower_arg_to` 直後の `last_arg_wrapped`(`T→T?` wrap 等で新セルが鋳造された場合)を OR、 `fresh_arg_needs_post_release(vty)` が真なら呼び出し後に Release する。 借用(変数)引数は対象外なので過剰解放しない。
- **同族探索**: `lower_arg_to` 使用箇所を全 grep。 leak したのは上記 2 経路のみ。 `lower_com_iface_dispatch`([object.rs](../crates/ilang-mir/src/lower/calls/object.rs):124)と c_vtable も引数解放を持たないが、 これらは C ABI 越しの外部メソッドで ilang heap object を渡す経路が稀かつ所有権規約が異なる(ilang で deinit を観測できない)ため対象外とした。
- **検証**: 修正後、 単発 deinit=2・churn delta=0/deinits=200(iface_arc probe)、 fn-field deinit=1/delta=0。 field 格納する method(`stash(b){ this.sink.held=b }`)は deinit=2/delta=0 で過剰解放なし・`ILANG_HEAP_GUARD=1` クリーン、 borrowed 変数引数は deinit=1/delta=0 で解放されない。 child-interface 受信側で親メソッド(inherited slot)に fresh 引数を渡す形も健全。 ilang-mir(lowering)を触ったので全儀式実施: workspace nextest 539/539、 AOT 全 program fixtures PASS、 nested_generic 100 並列 0 fail。 fixture: `09_subtyping/interface_dispatch_fresh_arg_release.il`(厳密 deinit tally + churn delta=0 + borrowed 非解放)。 docs syntax は ARC 内部規約のため変更なし。

### [解決済み記録] 第 91 弾: 再帰的 generic class で monomorphizer がハング (2026-06-14)

未踏の深い generic 入れ子/再帰を probe して発見。

- **症状**: `class Wrap<T> { pub v: T  init(v: T) {...}  doubled(): Wrap<Wrap<T>> { new Wrap<Wrap<T>>(new Wrap<T>(this.v)) } }` を書き `let w = new Wrap<i64>(7)` を作ると、 **doubled() を一切呼ばなくてもコンパイラがハング**(8 秒経っても終わらない)。
- **原因**: class monomorphization([class.rs](../crates/ilang-mir/src/monomorphize/class.rs))は eager — instantiate した class の全メソッドの body/戻り型を scan して更なる generic ref を worklist に積む。 `Wrap<i64>` の `doubled()` 戻り型 `Wrap<Wrap<i64>>` → 積む → その `doubled()` が `Wrap<Wrap<Wrap<i64>>>` → … と**ストリクトに深くなる**無限連鎖。 各レベルは distinct な mangled 名なので `synthesized`/`needed` の dedup が効かず worklist が永久に drain しない。 Rust/C++ も同型の無限 monomorphization を recursion limit でエラーにする。
- **修正**: drain ループに `const MONO_CLASS_LIMIT: usize = 1_000;` の count guard を追加。 `synthesized.len()` が超えたら明確な panic(「monomorphization limit exceeded (1000 …) — recursively instantiated at ever-deeper type arguments (e.g. a method returning `Wrap<Wrap<T>>`)」)。 `monomorphize` は `Program` を返す infallible 関数(main から直接呼ばれ Result 化は 5 関数+main の大改修)なので、 proportionate に panic を選択(第 72 弾の「ハング/未定義より clean なエラー」方針)。 ハング→~1 秒で終了。 limit は realistic な ilang プログラム(distinct instantiation 数 < 数百)の十分上。
- **検証**: runaway が ~1 秒で明確エラー、 全 539 test は 1000 instantiation を超えず無変更で緑(false-positive なし)。 valid な深い入れ子(`Box<Box<Box<i64>>>`)・generic enum 再帰(`List<T>`)は無影響で動作。 fixture: `05_edge_cases/mono_recursion_limit_error.il`(expect-error)。 ilang-mir(monomorphize)を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 program fixtures harness JIT + AOT PASS、 nested_generic 100 並列 0 fail。 (将来 lazy(呼ばれたメソッドのみ)instantiation に変えれば、 この特定形は limit に当たらず終了できる — 別途の改善余地。)

### [解決済み記録] 第 90 弾: const の div/mod by zero がコンパイル時に検出されず runtime panic (2026-06-14)

未踏の const 評価を probe して発見。

- **症状**: `const X: i64 = 10 / 0`(および `10 / (5-5)`・`10 % 0`・`1 << 70`)がコンパイルを通り、 モジュール init 時に **runtime panic**(`panic: division by zero`)。 `const_div_zero_error.il` fixture は `// expect-error: division by zero` で**コンパイル時検出を意図**しているが、 panic message も "division by zero" を含むため substring 一致で通っていた(意図が enforce されていなかった)。
- **原因**: const folder([consts.rs](../crates/ilang-parser/src/loader/consts.rs))の `fold_const_expr`(parser/loader 段)が `fold_binary` で div/mod by zero を検出して `Err(String)` を返すが、 caller(`process_const` と static field 経路)が **全 fold エラーを「非定数式 → runtime init へ降格」** 扱いし `_reason` を捨てていた。 「非定数(call/field/control-flow → runtime OK)」と「定数だが不正(div-by-zero → compile error)」を区別していなかった。 over-width(`const X: u8 = 300`)は fold 成功後の別チェックなので compile error になっていたが、 div-by-zero は fold 失敗 → runtime 降格だった。
- **修正**: fold エラーを 2 種に分ける enum `FoldErr { NotConst(String), Invalid(String) }` を導入。 `fold_const_expr`/`fold_binary`/`cast_const` の戻り値を `Result<Expr, FoldErr>` に。 div/modulo by zero・範囲外シフトを `Invalid`(畳み込めるが静的に不正)、 その他全エラーを `NotConst`(従来の runtime fall-through 維持)に分類。 両 caller が `Err(FoldErr::Invalid(reason))` を `LoadError::BadConst`(hard compile error)に、 `Err(FoldErr::NotConst(_))` を従来どおり runtime 降格に。
- **検証**: `10/0`・`10/(5-5)`・`10%0`・`1<<70`・static field `1/0` が明確なコンパイルエラー、 valid const 畳み込み(`A*2+5`=25)・非定数 const(`f()`)の runtime 降格は無変更。 既存 `const_*` fixture 全て緑(`const_div_zero_error` は今や正しく compile error で通る)。 fixture: `05_edge_cases/const_modulo_zero_error.il`(compile 特有の「in const expression」サフィックスを検査 = runtime panic と区別)。 parser/loader を触ったので AOT 儀式も実施。 workspace nextest 539/539、 program fixtures harness JIT + AOT PASS。 docs は const fold が `/ %` を受けると記載するのみで挙動詳細なし、 変更不要。

### [解決済み記録] 第 89 弾: 動的配列 static フィールドが checker を通り codegen で crash (2026-06-14)

未踏の static フィールドを probe して発見。

- **症状**: `class C { static arr: i64[] = [1,2,3] }` が checker を通り(メッセージも「dynamic arrays of numeric primitives は許可」と明記)、 宣言+init のみなら compile も成功するが、 `C.arr` を読む / `C.arr = [...]` で再代入すると「mir-codegen: unsupported in M1: static slot type」、 init-then-use(`C.arr.length` の後に再代入して読む)では **SIGSEGV**。 一方 object static は checker で明確に拒否、 string static は動作。
- **原因**: checker([sigs.rs](../crates/ilang-types/src/checker/sigs.rs))の static 許可型に `array_of_prim_ok`(numeric primitive の動的配列)が含まれるが、 codegen の `LoadStatic`/`StoreStatic`([static_slot.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/static_slot.rs))は単語値(I*/U*/F*/Bool/Str)しか扱わず `_ => Unsupported`。 string は heap ポインタだが単語として load/store され動くのに、 動的配列は対象外。 checker の意図(コメントが ARC retain/release 共有を謳う)に codegen が追いついていない半実装。
- **修正の試行と判断**: codegen で `MirTy::Array` を Str 同様 `raw`/`v` として Load/Store に追加してみたが、 init/ARC がより複雑で **SIGSEGV**(完全対応は Load/Store 追加だけでは不足)。 リスクが高く不確実なので codegen 変更は撤回し、 **保守的に checker 側で動的配列 static を拒否**(`array_of_prim_ok` を削除、 メッセージを「numeric primitives, bool, string」に修正)。 これで late crash が clean な checker 診断になる。
- **検証**: `static arr: i64[]` が明確な診断で拒否、 数値/bool/string static は無変更で動作(`Counter.total` 等の既存 fixture 緑)。 既存 fixture に動的配列 static を使うものは無し(workspace 539/539 で確認)。 fixture: `05_edge_cases/static_array_field_error.il`(expect-error)。 checker を触ったので AOT 儀式も実施。 workspace nextest 539/539、 program fixtures harness JIT + AOT PASS。 (将来 codegen が heap-array static slot を実装したら checker の許可を戻せる。)

### [解決済み記録] 第 88 弾: float リテラルが整数 SIMD レーンに無検査で通る (2026-06-14)

未踏の SIMD を probe して発見。

- **症状**: スカラ `let x: i32 = 1.0` / `let x: i32[] = [1.0]` は「expected i32, got f64」で拒否されるのに、 `let v: simd.i32x4 = [1.0, 2.0, 3.0, 4.0]`(整数レーンに float リテラル)はコンパイルできた。 SIMD はレーンアクセス・演算が未公開なので値は観測できないが、 公開時に garbage を生む型 soundness の漏れ(ilang の no-implicit-float-to-int 規則違反)。
- **原因**: SIMD 構築の literal 検証([mod.rs](../crates/ilang-types/src/checker/mod.rs))が `let dummy_vt = lane_ty.clone()` として、 各要素の「自身の型(vt)」に **lane 型を渡していた**。 そのため `literal_assignable_with(e, i32, i32)` となり全要素が自明に fit。 配列ケース(直上)は `vt`(値の実型 `Array { elem }`)の要素型を渡しており、 これと非対称だった。
- **修正**: 配列ケースと同形に、 `vt` から要素型を取り出して `literal_assignable_with(e, &vt_elem, &lane_ty, is_sub)` を呼ぶ。 float リテラル(f64)→ 整数レーン(i32)は narrow 不可で拒否、 int リテラル → 整数レーンは narrow 可、 float リテラル → float レーンは f64→f32 coerce、 int リテラル → float レーンは int→float で許可。 副次的に over-wide 整数リテラル(`300` を i8 レーンへ)も literal-fits 検査で拒否されるように。
- **検証**: `simd.i32x4 = [1.0,...]` / `simd.i8x16 = [300,...]` が拒否、 valid 構築(`i32x4` from int、 `f32x4` from float/int、 `i64x2` from int)・SIMD 配列・関数受け渡しは無変更で動作。 既存 `simd_vector_types.il` 緑。 fixture: `04_modules/simd_int_lane_float_literal_error.il`(expect-error)。 checker を触ったので AOT 儀式も実施。 workspace nextest 539/539、 program fixtures harness JIT + AOT PASS。 docs は元から「各要素はレーンのスカラ型に収まる」と正しく記載。

### [解決済み記録] 第 86 弾: `@extern(C) struct` の field-type 検証が抜けていた (leak/panic) (2026-06-14)

第 84 弾は struct の @bits だけ追加した。 残る field-type 検証の漏れを probe して是正。

- **症状**: `@extern(C) { struct S { x: u32  b: Box } }`(非 repr-c heap object フィールド)がコンパイルでき、 `s.b = new Box(7)` の Box が永久に解放されず **leak**(churn delta=2400, deinit=0)。 非最後の動的配列フィールドは使用時に **runtime panic**(overflow)。 tuple / optional / plain(非 repr)enum フィールドも通る。 これら inline レイアウト不可の heap 型は `check_class` が拒否するが struct 経路は素通り。
- **原因**: 第 84/85 弾と同根。 field-type の `ok` 検証は `check_class` の repr-C field ループにあり、 `ExternCItem::Struct` 経路(第 84 弾で @bits だけ共有)は型検証を欠いていた。
- **修正**: class 経路の field-type `ok` ロジックを `repr_c_field_ok(ty, is_last)` + `check_repr_c_struct_field(name, f, is_last)` メソッド([decls.rs](../crates/ilang-types/src/checker/decls.rs))に抽出し、 `check_class` と extern_c の Struct arm([extern_c.rs](../crates/ilang-types/src/checker/extern_c.rs))で共有。 抽出に際し許可集合を struct の実態に合わせて補正: **(1) repr enum を許可**(`enum E: u32` フィールド = `repr_c_flags_enum_field.il`、 旧 class `ok` は未対応)、 **(2) C スカラ `char` / `size_t` / `ssize_t` を許可**(`extern_slice_return.il` の `len: size_t`、 旧 `primitive_ok` が欠いていた — 開発中にこの 2 つを一度落として回帰、 既存 fixture が捕捉して補正)。 plain enum は heap-boxed なので拒否(repr enum のみ inline 安全)。
- **検証**: 非 repr-c object/tuple/optional/plain enum/非最後動的配列を明確な診断で拒否、 repr enum・C スカラ・nested repr-c struct・FAM(最後の動的配列)・raw ptr・fn ptr は維持。 既存 `repr_c_*` / `extern_slice_return`(harness 経由)全て緑。 fixture: `04_modules/extern_struct_heap_field_error.il`(expect-error)。 checker を触ったので AOT 儀式も実施。 workspace nextest 539/539、 program fixtures harness JIT + AOT PASS。 repr enum 許可は class 経路にも及ぶ(permissive・正しい)。

### [解決済み記録] 第 85 弾: `@extern(C) union` のフィールド検証が抜けていた (SIGSEGV) (2026-06-14)

第 84 弾(struct の @bits 検証漏れ)を直した直後、 同じ `ExternCItem` 経路の Union を probe して同根バグを発見。

- **症状**: `@extern(C) { union U { a: u64  b: Box } }`(heap object を union フィールドに)がコンパイルでき、 `u.b = new Box(42); u.a = 999; u.b.n` で **SIGSEGV (exit 139)** — union は offset 0 共有なので `a` への書き込みが `b` のポインタを上書きし、 stale な `999` を Box ポインタとして deref して crash。 string フィールドも同様。 docs / `check_class` は union の heap フィールドを「shared storage で危険」と拒否する。
- **原因**: 第 84 弾と同根。 union の heap-field 拒否・非空・@bits 拒否の検証は `check_class`([decls.rs](../crates/ilang-types/src/checker/decls.rs))の `if c.is_union` ブロックにあるが、 `@extern(C) { union ... }` は **`ExternCItem::Union`** にパースされ、 pass 2(`check_extern_c_bodies`)の match が Class のみ処理し Union は `_ => {}` で素通り。 検証が完全に欠落。
- **修正**: union 検証を free 関数 `validate_union(name, fields, span)` に抽出([decls.rs](../crates/ilang-types/src/checker/decls.rs))、 `check_class` の `if c.is_union` ブロックをこれの呼び出しに置換。 `check_extern_c_bodies` に Union arm を追加し `validate_union` を呼ぶ([extern_c.rs](../crates/ilang-types/src/checker/extern_c.rs))。 (第 84 弾の `validate_bitfield` と同じ「検証を free 関数に切り出して両経路で共有」パターン。)
- **検証**: heap union フィールド(object/string)・空 union が明確な診断で拒否(SIGSEGV 解消)、 valid な数値 union(`union { a: u32  b: f32 }` の type punning で `1.0`)は無変更で動作。 既存 `repr_c_union` 緑。 fixture: `04_modules/extern_union_heap_field_error.il`(expect-error)。 checker を触ったので AOT 儀式も実施(lowering 未変更のため nested_generic は省略)。 workspace nextest 539/539、 program fixtures harness JIT + AOT PASS。 docs は元から制約を正しく記載。

### [解決済み記録] 第 84 弾: `@extern(C) struct` の `@bits` 検証が抜けていた (2026-06-14)

`@crepr`/bitfield 周辺を probe して発見。

- **症状**: `@extern(C) { struct S { @bits(4) x: i32 } }`(符号付き bitfield)がコンパイルでき、 `x = -1` を書くと `0b1111` が格納され、 読むと符号拡張されず **15** になる(期待 -1)。 docs は `@bits(N)` を「unsigned 整数型のみ(u8/u16/u32/u64)・幅は `1..=型幅`」と明記。 `@bits(40) x: u32`(40 > 32 の幅超過)も通る。
- **原因**: `@bits` の signed/width 検証は `check_class`([decls.rs](../crates/ilang-types/src/checker/decls.rs))の repr-C field ループにあるが、 これは `ExternCItem::Class` 経路専用。 `@extern(C) { struct ... }` は **`ExternCItem::Struct`** にパースされ、 pass 1 で signature 登録(`class_signature`)されるだけで、 pass 2(`check_extern_c_bodies`)の match は Class のみ処理し Struct は `_ => {}` で素通り。 つまり bitfield 検証が完全に欠落。 パーサは `@bits(N)` の `1..=64` グローバル境界のみ検査(型ごとの幅・符号は未検査)。
- **修正**: class 経路のインライン bitfield 検証を free 関数 `validate_bitfield(struct_name, f)` に抽出([decls.rs](../crates/ilang-types/src/checker/decls.rs))、 `check_class` はこれを呼ぶ形に置換。 `check_extern_c_bodies` の Struct arm を追加し各 field に `validate_bitfield` を適用([extern_c.rs](../crates/ilang-types/src/checker/extern_c.rs))。 **当初 full `check_class(&synth)` を呼んだら** struct 固有の許可型(@flags enum フィールド = `repr_c_flags_enum_field.il`)を class の field 規則が誤拒否し回帰したため、 共有するのは **@bits 検証のみ**に絞った(field-type 規則は class と struct で正当に異なる)。
- **検証**: 符号付き bitfield・幅超過 bitfield が明確な診断で拒否、 valid な unsigned bitfield(`@bits(4) x: u32`)は無変更で動作。 既存 `repr_c_bitfield` / `repr_c_packed`(harness 経由)/ `repr_c_flags_enum_field` 全て緑。 fixture: `04_modules/extern_struct_bitfield_signed_error.il` / `extern_struct_bitfield_overwidth_error.il`(expect-error)。 checker を触ったので AOT 儀式も実施(lowering 未変更のため nested_generic は省略)。 workspace nextest 539/539、 program fixtures harness PASS、 AOT 全 fixture PASS。 docs は元から制約を正しく記載していたので変更なし(第 71/72 弾と同型: docs の契約に実装が追従)。

### [解決済み記録] 第 81 弾: join 共変を map / tuple リテラルに拡張 (2026-06-14)

第 79/80 弾の継ぎ目を probe して残る fresh-literal container の取りこぼしを発見、 同原理の完成として実装。

- **症状**: `fn f(b): Map<string, Animal> { if b { {"k": new Dog()} } else { {"k": new Cat()} } }` が「expected Map<string, Dog>, got Map<string, Cat>」、 `fn g(b): (Animal, i64) { if b { (new Dog(), 1) } else { (new Cat(), 2) } }` が「expected (Dog, i64), got (Cat, i64)」で拒否。 単腕の map/tuple リテラル共変(docs 記載)は通るのに join だけ非対称。
- **判断**: 第 80 弾でユーザーが「fresh literal 共変を join に広げる」を承認済み(配列・some)。 map/tuple も同じ fresh composite literal・同じ mutable・literal-only の健全性プロファイルなので、 第 80 弾の決定の自然な完成として再相談せず実装(別の type/構文を導入するわけではない)。
- **修正**: `common_generic_join`([utils.rs](../crates/ilang-types/src/checker/utils.rs))に `Type::Tuple`(同 arity、 各要素を `join_type_arg` で共変 join)を追加。 Map は `Type::Generic{base:"Map", args:[K,V]}` なので既存の Generic arm で V を共変 join 済み — gate だけが必要だった。 `is_covariant_join_literal` に `ExprKind::MapLit` / `ExprKind::Tuple` を追加、 `covariant_widening` に Tuple を追加(同サブクラス両腕の境界 widening 用)。 **literal-only ゲートは全発火点で維持** — 別名 map/tuple を含む腕の join は不変。
- **検証**: map/tuple の if/match join が値正・多型、 ARC churn は delta=0・deinit 厳密(map 値 / tuple 要素の heap payload が leak なし)。 別名 map join・別名 tuple join・numeric narrowing は拒否、 既存 alias 負テスト 2 件も維持。 fixture: `09_subtyping/generic_enum_covariant_join.il`(map / tuple ケース追加、 JIT / AOT PASS)。 checker を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 docs(syntax.md / syntax_ja.md)を map/tuple 込みに更新。

### [解決済み記録] 第 80 弾: join 共変を配列リテラル / some 包みに拡張 (2026-06-14)

第 79 弾(generic enum の join 共変)の継ぎ目を probe して同族の取りこぼしを発見、 ユーザー決定で拡張。

- **症状**: `fn f(b): Animal[] { if b { [new Dog()] } else { [new Cat()] } }` が「expected Dog[], got Cat[]」、 `fn g(b): Box<Animal>? { if b { some(Box.hold(new Dog())) } else { some(Box.hold(new Cat())) } }` が「expected Box<Dog>?, got Box<Cat>?」で拒否。 単腕 `let a: Animal[] = [new Dog()]`(配列リテラル共変、 docs 記載)と素オブジェクトの some 合流(`some(new Dog())`/`some(new Cat())` → `Animal?`)は通るのに、 配列・some 包みの join だけ非対称。
- **判断**: 配列は enum と違い mutable で共変の健全性が微妙(ただし literal-only なら別名が無く健全)。 仕様判断としてユーザーに ilang コードで提示し、 **配列リテラルと some 包みの両方に広げる**決定を得た。
- **修正**: `common_generic_join`([utils.rs](../crates/ilang-types/src/checker/utils.rs))を Generic に加え `Array`(同 length-kind、 elem を `join_type_arg` で共変 join)と `Optional`(inner を共変 join)も扱うよう一般化。 `is_covariant_join_literal` に `ExprKind::Array`(新鮮な配列リテラル)と `ExprKind::Some(inner)`(inner が eligible なら)を追加。 境界 widening の `covariant_widening` は既に Array / Optional を含む。 **literal-only ゲートは全発火点で維持** — 別名配列を含む腕の join は不変のまま(混在腕の負 probe で確認)。
- **検証**: 配列の if/match join・Dog/Dog 配列・some 包み generic enum の join が全て正、 多型読み出しも正。 配列 join の ARC churn は delta=0・deinit 厳密(leak / over-retain なし)。 別名の直接代入・混在腕 join・numeric narrowing は従来どおり拒否、 既存 alias 負テスト 2 件も維持。 fixture: `09_subtyping/generic_enum_covariant_join.il`(array / opt ケース追加、 JIT / AOT 両経路 PASS)。 checker を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 docs(syntax.md / syntax_ja.md)の covariance 節を配列・some 込みに更新。

### [解決済み記録] 第 79 弾: generic enum covariance が if/match join を通らない (2026-06-14)

型推論の継ぎ目(第 59-67 弾の generic enum covariance)を probe して発見。 ユーザーに「join に広げるか / 現状維持か」を ilang コードで提示し、 **広げる**判断を得て実装。

- **症状**: `enum Box<T> { hold: (T) }`、 `Dog`/`Cat <: Animal`。 単腕 `fn f(): Box<Animal> { Box.hold(new Dog()) }` は covariant に通るのに、 `if b { Box.hold(new Dog()) } else { Box.hold(new Cat()) }: Box<Animal>` は「expected Box<Dog>, got Box<Cat>」、 同サブクラス両腕は「body produces Box<Dog>」で拒否。 plain object の if 合流(`if b { new Dog() } else { new Cat() }: Animal`)は共通祖先 `Animal` を計算して通るので、 generic enum だけ非対称だった。
- **原因**: `assignable`(ops.rs、 free fn でクラス階層を知らない)は generic を invariant 扱い(`assignable(Box<Dog>, Box<Animal>)` = false)。 単腕が通るのは `refine_enum_ctor_args` が ctor literal の型引数を期待型へ書き換えるため。 if/match の join(`check_if_expr` / `unify_branch_obj`)は両腕の型を bottom-up に merge するだけで、 `merge_generic_with_holes`(Any 穴のみ)・`common_object_join`(Object↔Object のみ)では `Box<Dog>` ⊔ `Box<Cat>` を join できず Mismatch。
- **修正**: ([utils.rs](../crates/ilang-types/src/checker/utils.rs))に同 base generic を各 arg covariant に join する `common_generic_join`(Object arg は `common_object_join` で共通祖先)、 境界 widening 用 `covariant_widening`(numeric narrowing は除外)、 literal 判定 `is_covariant_join_literal`(EnumCtor / if / match / Block を再帰)を追加。 `check_if_expr`([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs))と match の 3 つの `unify_branch_obj` 呼び出しに配線、 fn 戻り([decls.rs](../crates/ilang-types/src/checker/decls.rs))と let([stmt.rs](../crates/ilang-types/src/checker/stmt.rs))の境界に gated `covariant_widening` を追加。 **すべて `is_covariant_join_literal` でゲート**し、 全腕が ctor literal のときだけ共変させる(別名 generic 変数は不変 = literal-only を厳守)。
- **健全性の罠**: 最初は型のみの判定で実装したため、 alias 負テスト 2 件(`generic_enum_literal_covariant_alias_error.il` / `map_literal_covariant_value_alias_error.il` — 「別名 `Box<Dog>` 変数は `Box<Animal>` へ代入不可」)を壊した(checker が誤って受理し MIR lowering で crash)。 literal/alias を式で区別する `is_covariant_join_literal` ゲートを全発火点に入れて是正。
- **検証**: if/match の Dog/Cat literal join → `Box<Animal>`、 同サブクラス両腕、 多型読み出し、 2 型パラメータ(`Either<Animal, i64>`)が全て正。 別名の直接代入・別名の if-join は従来どおり拒否、 numeric narrowing(`fn f(): i8 { 200 }`)も拒否維持。 既存 covariance / 推論 fixture 全て無変更で緑。 fixture: `09_subtyping/generic_enum_covariant_join.il`(JIT / AOT 両経路 PASS)。 checker を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 docs(syntax.md / syntax_ja.md)の covariance 節に join 共変を追記。

### [解決済み記録] 第 77 弾: `Set` の Optional wrap が return 位置で早期解放 (2026-06-14)

第 76 弾(strong→weak? の wrap 漏れ)と同じ家系を別の inner 型で probe して発見。

- **症状**: `fn f(): Set<Box>? { let s = new Set<Box>(); s.add(new Box(3)); s }`(bare `Set → Set?` wrap を return)を呼ぶと、 `setSize(f())` が **0**(set が空/dangling)。 `let os: Set<Box>? = s` と明示 `some(s)` は正しく読める。 ARC のカウント自体は合っていた(churn deinits は期待通り)ので、 値だけが壊れる silent な破綻。
- **原因**: `retain_if_heap`([utils.rs](../crates/ilang-mir/src/lower/utils.rs))の heap 判定が canonical な `MirTy::is_heap`(Str/Object/Weak/Enum/Array/Tuple/Optional/Map/Set/Promise/Fn)の手書きコピーで、 **`Set` / `Weak` / `Promise` を欠いていた**。 `coerce` の `T → T?` wrap 腕([coerce.rs](../crates/ilang-mir/src/lower/coerce.rs))がこれを使うため、 `Set` を Optional に包むとき retain が出ない。 source が scope に残る let 位置では読めるが、 return 位置では source local の scope-exit release が set を解放 → 返った Optional が解放済み set を指し size=0。 (`fresh_arg_needs_post_release` のコメントが「per-site コピーが diverge して Promise を欠き leak した」と既に注記しており、 同じ diverge の別インスタンス。)
- **修正**: `retain_if_heap` を `ty.is_heap()` 委譲に変更(単一情報源化、 今後 diverge しない)。 `Inst::Retain` は codegen で型ごとに `__retain_set` / `__retain_weak` / `__retain_promise` 等へ dispatch 済みなので、 全 heap shape を retain して安全。
- **検証**: return / let / field / arg / closure capture / 明示 some の全配置で `Set<Box>?` の値が正しく(size 一致)、 churn delta=0・deinit 厳密(over-retain も leak もなし)。 `Weak → Weak?`(既に weak な値の wrap)・`Promise` も均衡。 既存の weak / collection / closure fixture 全て無変更で緑。 fixture: `05_edge_cases/optional_wrap_set_retain.il`(JIT / AOT 両経路 PASS)。 lowering を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 76 弾: strong → `Node.weak?` の bare coercion が配置ごとに破綻 (2026-06-14)

weak 参照を array / map値 / enum payload / tuple に入れる組合せ(既存 fixture は単一オブジェクトの back-ref のみ)を網羅 probe する中で発見。 weak × コンテナ自体は全て健全だったが、 strong を `Node.weak?` に入れる暗黙 coercion に複数の穴があった。

- **症状(配置で挙動が違う)**:
  - `let w: Node.weak? = strongRef` / `Node.weak?` 引数 → MIR lowering エラー「no coercion from obj#0 to weak#0?」(コンパイル不可)。
  - `obj.f = strongRef`(`Node.weak?` field)→ コンパイルは通るが、 素の strong ポインタを weak Optional スロットに格納し、 後の `.get()` / release で **SIGSEGV**。
  - `fn f(k: Node): Node.weak? { k }`(borrowed strong を return)→ **leak**(deinit 不発)。 plain `fn f(k: Node): Node.weak { k }` でも同じく leak(**既存バグ**、 git stash で HEAD でも再現確認)。
  - 唯一 `let w: Node.weak? = some(strongRef)`(明示 some)だけが正常(end-to-end で生存読み・死後 none・deinit 厳密)。
- **原因**: `Object → Optional<Weak>` は「strong→weak downgrade」と「T→T? wrap」の合成。 `coerce` ([coerce.rs](../crates/ilang-mir/src/lower/coerce.rs)) の wrap 腕の `obj_shape` も Optional→Optional widen の `is_obj_shape` も **Weak を含まず**、 どの規則にもマッチせず fallthrough。 明示 `some` は `lower_some_with_hint` が inner を先に Weak へ coerce してから wrap するので回避できていた。 field store は `store_value_to_field` ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)) の `needs_optional_wrap` / `needs_strong_to_weak` がどちらも `Optional<Weak>` を捉えず、 素の値を格納していた。 return leak は `emit_callee_retain` ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs)) が borrowed tail を **strong** retain した後、 `finalise_return` が StrongToWeak へ coerce するため、 その strong +1 が返り値(weak)に乗らず孤児化。 `release_owned_wrap_source` は `tail_owned=false`(借用 tail)で発火しない。
- **修正(3 箇所)**:
  - `coerce`: `Object → Optional<Weak>` を `lower_some_with_hint` と同じ手順で分解 — inner を `Weak` へ coerce(StrongToWeak)、 `Inst::Retain`(weak 型なので `__retain_weak`)で Optional の weak-rc share を取り、 `NewOptional`。
  - `store_value_to_field`: `needs_optional_wrap` 条件に `Optional<Weak>` ← `Object` を追加し coerce 経路へ流す。
  - `emit_callee_retain`: 返り値型が `Weak` / `Optional<Weak>` かつ tail が `Object` のとき strong borrow-retain を **抑制**。 weak 側は自前で rc を均衡させる(bare `let w: Node.weak = s` が retain 無しで delta=0 になるのと同じ原理)。 これで plain `Node.weak` 返却の既存 leak も同時に解消。
- **検証**: 全配置 — let / 引数 / field(生存=値、 死後=none)/ return / tuple 要素 / array 要素 / map 値 / enum payload — で正しい値・死後 none・churn delta=0・deinit 厳密。 weak param をそのまま返す形 (`fn(w: Node.weak): Node.weak { w }`) も delta=0。 既存 weak fixture(arc_cycle_via_weak / leak_weak_get_loop / weak_backref_cascade_release_order / leak_optional_weak_field_reassign / early_exit_weak_property_iface)全て無変更で緑。 `tail_owned` は触っていないので `Dog → Animal?` 等の wrap-return 経路に無影響。 fixture: `05_edge_cases/optional_weak_from_strong.il`(JIT / AOT 両経路 PASS)。 lowering を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 docs は `Node.weak?`・strong→weak downgrade・T→T? wrap を既に記載しその合成が動くようになっただけなので変更なし。

### [解決済み記録] 第 75 弾: `{}` map リテラルのクラスキーが値等価でキーされない (2026-06-14)

第 73/74 弾で見た「ポインタ同一性 vs 値の比較」という同族の落とし穴を、 enum からクラスへ広げて probe して発見。

- **症状**: `@derive(Eq, Hash) class Point` を `let m: Map<Point, V> = {}` (空 map リテラル) のキーにすると、 `equals`/`hashCode` は正しく合成・動作する (`p.equals(q)` true、 hash 一致) のに、 `m.has(new Point(1,2))` / `m.get(...)` が **値等価な別インスタンスを miss** し、 同論理キーの再代入も上書きされず `size` が 2 のまま残る。 一方 **`Set<Point>` は正しく dedup する** (この非対称が手掛かり)。 `new Map<Point, V>()` 構築子経由なら正常。
- **原因**: `new Map<MyClass, V>()` ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)) は object キーを検出して `$map.newObject(equals, hashCode)` を MIR レベルで配線するが、 map リテラルは `Inst::NewMap` に lowering され、 その codegen ([map_inst.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/map_inst.rs)) は **キー型に関係なく常にプリミティブ `__map_new` を呼ぶ** — eq/hash 関数を渡さないので Int store がボックスのポインタをキーにする。 `Set` はリテラル構文が無く常に `new Set<>()` 経由で object 構築されるため影響を受けなかった。
- **修正**: `new Map<>()` の object 構築 (`$map.newObject` + print-kind / val-kind タグ付け) を `build_object_keyed_map(class_id, val, ty)` ヘルパー ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)) に抽出。 `{}` リテラル経路 ([literals.rs](../crates/ilang-mir/src/lower/literals.rs) の `lower_map_literal_with_hint`) で key_ty が `Object(class)` のとき同ヘルパーで構築し、 (理論上の) エントリは `map_set` builtin で挿入。 map リテラルはキーにリテラルトークンしか書けず、 クラスインスタンスを綴れないので実害は空 `{}` 形のみだが、 それが注釈付き空 map の慣用記法。
- **検証**: `{}` 経由の `Map<K, string>` / `Map<K, K>` で has/get/delete/上書きがすべて値等価基準、 `console.log` がキーを構造表示、 通常キー (`Map<string, i64> = {}`、 非空 string リテラル) は無変更で回帰なし。 ARC: object キー map literal の churn で deinit 厳密 (key+value で round あたり 2、 100 round で 200)・delta=0 (leak / over-release なし)。 fixture: `03_collections/map_object_key_brace_literal.il` (JIT / AOT 両経路 PASS)。 MIR lowering を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 docs は元から「値等価プロトコルは Set / Map の両方に効く」と正しく記載済みなので変更なし。

### [解決済み記録] 第 74 弾: repr enum 同士の順序比較がポインタ比較で誤答 (2026-06-14)

第 73 弾 (`==` のポインタ比較) の同族を探索して発見。 第 73 弾の修正後、 自分が触った継ぎ目 (cast / 等価) を repr enum で網羅 probe する過程で順序比較に落とし穴を見つけた。

- **症状**: `Msg.paint < Msg.quit` (`enum Msg: u32`、 15 < 16) が **false**、 `Msg.quit < Msg.paint` (16 < 15) が **true**。 `Msg.close > Msg.quit` (18 > 16) や enum-vs-int リテラル (`Msg.quit < 18`) は正しいので発見しにくい。 非 repr enum (`Color`) の `<` は型エラーで正しく拒否される。
- **原因**: repr enum はタグに自然順序があり checker が enum-vs-enum 比較を許可する (`Msg.quit < Msg.close` がコンパイル成功) が、 lowering ([ops.rs](../crates/ilang-mir/src/lower/ops.rs) の `lower_binary`) は両辺が同じ enum 型のとき昇格せず、 `cmp_op` を **ボックスのポインタ**に適用していた。 アドレス順は intern/alloc 順次第なのでビット値の順序と無関係。 d41ca1b5 の repr 昇格は enum-vs-**int リテラル**のときだけ発火し enum-vs-enum を救わない。 `==` (第 73 弾) は intern でポインタが canonical 化され、 同ビット → 同ポインタ → `IEq` が偶然正しく読めていたが、 **順序はポインタ順自体が無意味なので intern では救えない** (これが第 73 弾で見落とした穴)。
- **修正**: `lower_binary` の bitwise ブロック直後に「両辺が同型 enum の比較 (`==` `!=` `<` `<=` `>` `>=`)」分岐を追加。 両辺の tag を `Inst::EnumTag` で抽出し、 `==`/`!=` は `IEq`/`INe`、 順序は `cmp_op(repr_ty, ..)` (repr 型の符号で signed/unsigned を選択) で比較。 これで `==`/`!=` も intern 依存をやめ tag 抽出に統一 (より頑健)。 enum-vs-int リテラルは別経路 (片側 int) なので非干渉、 非 repr enum の順序は従来どおり checker が型エラーにする。 payload enum の `==` は checker が拒否するのでこの分岐には来ない (全 variant unit/flags/repr のみ)。
- **検証**: u32 repr の全順序演算子・`==`/`!=`・enum-vs-int 双方向、 i32 repr (負の discriminant `cold = -10`) の signed 順序がすべて正しい (unsigned だと `cold < mild` が誤る → 符号選択が効いている証拠)。 第 73 弾 fixture (`flags_enum_combined_identity`) と既存 enum fixture 全て無変更で緑。 fixture: `06_enums/repr_enum_comparison.il` (JIT / AOT 両経路 PASS)。 lowering を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 syntax.md / syntax_ja.md の repr enum 節に比較・順序の契約を追記。

### [解決済み記録] 第 73 弾: combined `@flags` 値の `==` / Set / Map / leak が同根で破綻 (2026-06-14)

`@flags` enum (第 68/69 弾で `has` / `~` を直した) の継ぎ目を ARC・等価・コレクションの全方位で probe して発見。 1 つの原因 (`$enum.box` が intern しない) が 3 症状に分岐していた。

- **症状 (3 つ、同根)**:
  1. `let a = read|write; let b = read|write; a == b` が **false** (同ビットなのに不一致)、 `a != b` が true。 名前付き単一変異 (`P.read == P.read`) と plain unit enum は正常 (singleton で偶然一致)。 マスク比較 `(f & write) == write` (box 左・cached 名前付き右) も同様に false。
  2. `Set<P>` に `read|write` を 2 回 add すると `.size()` が **3 でなく** dedup されず増える。 `Map<P,V>` も同一 combined キーを別物として keying。 (既存 `set_of_enum.il` は「異なるビット列を 3 個」入れるだけだったので pointer/tag どちらでも size=3 で通り、 露見しなかった。)
  3. combined 値を churn すると **線形 leak** (`read|write` 100 回で delta≈3264)。
- **原因**: `@flags` の bitwise / `~` / `int as Enum` cast は結果を `$enum.box` で re-box するが、 runtime `__enum_box` は毎回 `rc=-1` の新規セルを `__mir_alloc` していた。 一方 ilang は「enum 値はポインタ同一」を不変条件にしており (等価は `IEq` でボックスポインタ比較、 Set/Map の Int store は raw セル = ポインタを key にする)、 名前付き変異は `__enum_unit_get` の `(global_eid, disc)` intern キャッシュで canonical 化されるのに combined 値だけがこの不変条件を破っていた。 syntax.md は「equality は discriminant tag 上」「unit-variant / `@flags` enum は Set 要素 / Map キー可」と明記しており、 **意味論の選択ではなく実装が契約に追従していない**バグ。 rc=-1 セルは release が短絡するため永久に解放されず leak も発生。
- **修正**: `$enum.box` を **codegen の builtin intercept** で処理 ([calls.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/calls.rs) の `lower_call` 冒頭、 `console_log` 等と同じブロック)。 結果値の MIR 型 `MirTy::Enum(eid)` から local id を読み、 `enum_global` でグローバル id へ再マップ (`NewEnum` の unit 経路と同じ表)、 disc 引数を渡して `__enum_unit_get(global, disc)` を直接呼ぶ。 これで combined 値が名前付き変異と **同一の canonical ポインタ**になり、 `read|read == read` (box vs cached) も含めて等価・Set・Map が一致、 alloc も値ごと 1 セルに収束 (leak 解消)。 extern C 戻り値の enum を同じ intern 経路に通す既存処理 ([calls.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/calls.rs) line ~757) と完全に同型。 MIR lowering 側 ([ops.rs](../crates/ilang-mir/src/lower/ops.rs) の `~` / bitwise、 [coerce.rs](../crates/ilang-mir/src/lower/coerce.rs) の cast) は `$enum.box([disc])` を emit するまま無変更。 dead 化した runtime `__enum_box` / `program_decl` の宣言 / dispatch リスト / `jit_symbols` 登録を撤去。
- **検証**: 単発で 3 症状すべて解消 (`combined ==` true、 Set/Map dedup 正、 churn delta=0)、 `read|read == read` も true、 既存 `flags_enum_bitwise` / `flags_enum_has_and_not` / `set_of_enum` / `repr_enum_patterns` / `enum_match_invalid_value_panics` (第 72 弾の panic) すべて無変更で正常。 fixture: `06_enums/flags_enum_combined_identity.il` (JIT / AOT 両経路 PASS)。 codegen を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 docs は契約が既に正しかったので変更なし。

### [解決済み記録] 第 72 弾: 無効 enum 値の wildcard 無し match が SIGILL (2026-06-13)

panic 経路の継続 probe で発見:

- **症状**: `pub enum Msg: u32` に `let m = 99 as Msg` (宣言外の値) を作り、 `match m { quit { } close { } }` (全 variant 網羅・`_` 無し) にかけると **SIGILL (exit 132)**。 trace すると match 直前まで実行され、 マッチで illegal-instruction trap。 `_` wildcard 付き (Win32 ディスパッチ形) や valid な値なら正常。
- **原因**: repr enum は `n as E` で任意 int を cast でき宣言外の値を持てるが、 checker は全 variant 網羅の match を exhaustive と見なし `_` を要求しない。 その合成 default ブロックの terminator が `Terminator::Unreachable` ([match_.rs](../crates/ilang-mir/src/lower/match_.rs)) で、 codegen が `trap` (TrapCode) を emit する。 無効値がこの「到達不能」ブロックに到達 → SIGILL。
- **修正**: no-wildcard default で Unreachable の前に **クリーンな panic を emit**: `Inst::Const { MirConst::Str("panic: no matching enum variant") }` + `$ilang.panic` への builtin Call。 `$ilang.panic` は program_decl で declare 済みだが MIR builtin dispatch 未配線だったので、 codegen の `$xxx.yyy` 直接解決リスト (`$enum.box` 等と同じ、 [calls.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/calls.rs)) に `$ilang.panic` を追加。 非 repr enum では default は genuine に到達不能なので dead code (害なし)。
- **検証**: `classify(99)` が `panic: no matching enum variant` + exit 1 (SIGILL でなく)、 valid 値・`_` 付き・非 repr enum の match は無変更で正常。 fixture: `06_enums/enum_match_invalid_value_panics.il` (`// expect-error` でランタイム panic を pin)。 lowering + codegen を触ったので AOT・nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 71 弾: `m[missing]` が panic せず default を返す (2026-06-13)

panic / ランタイムエラー経路を probe して発見:

- **症状**: 配列 OOB read/write (`panic: index out of bounds`) ・`o.unwrap()` on none (`panic: unwrap of None`) ・ゼロ除算 (`panic: division by zero`) はいずれも明確に panic + exit 1 するのに、 **`m["missing"]` (Map の欠落キーを index `[]` で読む) が panic せず default を返す**。 i64 マップで 0 ・string マップで空文字 ・**object マップで null ポインタ** (後続の `b.n` 等で誤動作・出力すら出ない)。 syntax.md は `m["a"] // read (missing key panics at runtime)` と元から明記し、 `m.get(k): V?` が安全版。 つまり実装が docs/意図と不一致。
- **原因**: runtime `__map_get` ([maps.rs](../crates/ilang-runtime/src/maps.rs)) が欠落キーで `unwrap_or(0)` / `unwrap_or(&0)` し、 0 を返していた。 `MapGet` inst は borrowed read で、 codegen の OOB/div/unwrap のような panic チェックが無い (それらは codegen 側で条件分岐 emit)。
- **修正**: print.rs に **Rust 呼び出し可能な panic ヘルパー `rt_panic(&str) -> !`** を新設 (`__ilang_panic` と同じ出力形 = `<msg>\n` を stderr + exit 1)。 `__map_get` の 3 arm (Int/Str/Object) すべてで欠落時に `rt_panic("panic: key not found in map")` を呼ぶ。 `m == 0` (null マップ) は別扱いで 0 を維持。 `m.get(k)` (`__map_get_optional`) は無変更 (安全版)。
- **検証**: `m["nope"]` が `panic: key not found in map` + exit 1、 present key は無変更で正常。 既存 fixture は `m[missing]` の default 返しに依存しておらず全緑。 fixture: `05_edge_cases/map_index_missing_key_panics.il` (`// expect-error: key not found in map` でランタイム panic を pin、 `division_by_zero_int.il` と同形式)。 runtime を触ったので AOT 経路でも確認。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 69 弾: repr enum を int リテラルと比較できない非対称 (2026-06-13)

`@flags`/`@repr` enum のビット演算周辺を probe して発見:

- **症状**: `pub enum Msg: u32` で `Msg.close == 18` (enum == **int リテラル**) が「cannot apply binary op between Msg and i64」。 一方 `m == Msg.close` / `Msg.close == m` (m: u32 **変数**) は通る。 両方向のリテラル比較 (`18 == Msg.close` も) が失敗。 `Msg.close as u32 == 18` (明示キャスト) は回避できていた。
- **原因**: enum-repr promotion ([expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs)) は「enum の repr が**相手側の型に厳密一致**するとき」だけ enum を repr に promote する。 int リテラル `18` は既定で **i64** なので u32-repr とは一致せず promote されない。 さらに後段の「リテラル採用」は両辺が int のときだけ発火するが、 enum 側が Object のままなので発火せず、 `bin_result(==, Msg, i64)` が拒否。
- **修正**: promotion の match に「**repr enum vs repr に収まる int リテラル**」の 2 arm を追加 (`(Some(lr), None) if lr.is_int() && numeric_literal_fits(rhs, &lr)` とその左右逆)。 enum を repr へ promote し、 リテラルもその repr 型に採用する。 これで変数比較と同じ経路に乗る。
- **検証**: 両方向 (`Msg.close == 18` / `18 == Msg.close`) ・全比較演算 (`==`/`!=`/`<`/`<=`/`>`/`>`) ・変数比較 (回帰なし) ・tag 値の正しさを網羅。 fixture: `05_edge_cases/repr_enum_int_literal_compare.il`。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 68 弾: `@flags` enum の `has` 未 lower と `~` の SIGSEGV (2026-06-13)

`@flags` enum のビット演算を probe して発見した 2 件の lowering バグ:

- **(1) `f.has(other)` が lower 未実装**: checker ([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs):757) は `@flags` enum に `has(other): bool` (= `(f & other) == other`) を合成して受理するが、 MIR lowering に対応する handler が無く `method call on this type / unhandled builtin`。 既存 fixture (`flags_enum_bitwise.il`) は `|`/`&`/`^` のみで has 未テストだった。 修正: `try_lower_flags_method` ([calls/mod.rs](../crates/ilang-mir/src/lower/calls/mod.rs)) を method-call lowering の末尾 (Err 直前) に追加。 receiver と arg の tag を `Inst::EnumTag` で抽出、 `IAnd` して arg の tag と `IEq` 比較し bool を返す。
- **(2) `~f` (BitNot) が boxed 値を直接 NOT して SIGSEGV**: `lower_unary` の `BitNot` arm ([ops.rs](../crates/ilang-mir/src/lower/ops.rs):41) が flags enum の **boxed 値 (ポインタ) に `UnOp::Not` を直接適用**し、 結果を `MirTy::Enum` 型にしていた。 そのため `~Perm.read as u32` のような後続の tag 読み (`EnumTag`) がポインタを NOT した garbage を deref して **確定 SIGSEGV** (`~f as i64` で 139)。 `~f & flag` 経由は偶然動いていた。 修正: 二項 flags op と同形に、 `EnumTag` で tag 抽出 → `UnOp::Not` (i64) → `$enum.box` で再 box。 値は二項 ops と一貫 (`~read as u32` = 0xFFFFFFFE = 4294967294、 `~read & all` = write|exec = 6)。
- **検証**: has (基本 / compound flag / self / 空交差) ・ `~` (値 6 / 4294967294 ・ `~f` を has に再投入) を網羅。 fixture: `05_edge_cases/flags_enum_has_and_not.il`。 **両方とも flags enum の lowering 漏れ** (checker は元から正しい)。 lowering を触ったので nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 67 弾: enum covariance を入れ子降下 (some/tuple) でも効かせる (2026-06-13)

第 66 弾の covariance generalization を probe して発見した取りこぼし:

- **症状**: `let o: Result<Animal, string>? = some(Result.ok(new Dog()))` が「expected Result<Animal, string>?, got Result<Dog, any>?」で拒否。 第 66 弾の covariance は直接 let / 引数 / return / 配列 / Map では効くが、 **`some(..)` 包み**では効かなかった。
- **原因**: 第 66 は covariance を `value_assignable` (メソッド、 self あり) に実装したが、 `some(inner)` を Optional へ入れる際の降下は `literal_assignable_with` (自由関数、 self なし → enum sig にアクセス不可) で行われ、 そこに covariance が無かった。 配列/Map リテラルは hint-checker (`check_array_with_hint` 等) が要素ごとに `value_assignable` を呼ぶので元から効いていた。
- **修正**: vt (例 `Result<Dog,Any>`) が既に ctor の推論済み型引数を持つことを利用し、 **enum sig 不要の型レベル covariance** に置き換え。 `literal_assignable_with` ([mod.rs](../crates/ilang-types/src/checker/mod.rs)) の末尾に EnumCtor arm を追加: value が EnumCtor・vt/target が同 base の generic enum なら、 各型引数位置を `type_covariant_to` (equal / `Any` / Object subtype・interface 実装 / Array・Optional・Generic の構造的再帰) で検査。 `literal_assignable_with` は some/tuple/array/map の降下で自身を再帰呼びするので、 任意の入れ子で合成。 第 66 のメソッド版 `enum_ctor_literal_covariant` (sig + payload 式を使う版) は冗長になり削除。 refine 側の `is_covariant_upcast` (ctor を親型で記録) は据え置き (some 降下も `refine_enum_ctor_args` の Some arm で内側 ctor に届く)。
- **検証**: 引数 / return / `some(..)` 包み / tuple 要素 / 深い入れ子 (`Result<Animal,string>?[]`) を heap Dog payload + 親型越し vdispatch で網羅。 deinit 厳密 (400/round = 4/round × 100) + churn delta=0。 第 66 の直接 let / array / map と別名拒否も回帰なし。 fixture: `09_subtyping/generic_enum_covariant_nested.il`。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 66 弾: generic enum リテラルの covariance (2026-06-13、 ユーザー決定)

generic enum の covariance を probe して、 配列/Map リテラルは covariant なのに generic enum リテラルは非対応という不整合を発見。 ユーザー判断 (= 配列/Map と一貫させる) で対応:

- **背景**: `let r: Result<Animal, string> = Result.ok(new Dog())` が「type mismatch: expected Result<Animal, string>, got Result<Dog, any>」で拒否。 配列 (`Animal[] = [new Dog()]`) / Map リテラルは covariant (第 27/28 弾) なのに generic enum は非対応だった。 exact 一致 (`Result<Dog,string> = Result.ok(new Dog())`) は元から動く。
- **修正**: (1) `value_assignable` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) に `enum_ctor_literal_covariant` を追加 — value が EnumCtor リテラル・target が同 base の generic enum のとき、 各 payload 引数を「宣言 payload 型 (subst 後)」に対して `literal_assignable_with` で covariant 検査 (ok=T 位置・err=E 位置とも)。 (2) `refine_enum_ctor_args` の slot 置換に **subtype upcast** (`is_covariant_upcast`: Object subtype / interface 実装) を追加 — 推論 slot が Dog でも target が Animal なら Animal へ upcast し、 ctor が `Result<Animal,string>` として記録される (monomorphizer が親型を構築)。 payload の Dog は Animal slot にそのまま格納 (オブジェクト表現共有なので coerce 不要)。
- **リテラル限定で健全**: 別名 `Result<Dog,string>` 変数は `Result<Animal,string>` へ代入不可 (value が EnumCtor リテラルでないと covariance 句が効かない)。 enum は immutable 値なので親型越しの書き戻しが無く健全。
- **検証**: ok/err 両位置・let/array/map・heap payload (`Dog` deinit)・親型越し仮想ディスパッチ (`val()` → Dog.val()=10) を網羅。 deinit 厳密 (300/round = 3/round × 100) + churn delta=0。 別名拒否を expect-error で pin。 fixture: `09_subtyping/generic_enum_literal_covariant.il` + `..._alias_error.il`。 syntax.md / syntax_ja.md の共変節を更新。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 65 弾: generic メソッド引数の enum ctor が refine されず async fn が Result を返せない (2026-06-13)

async + Result を probe する過程で発見 (第 63 弾の診断が表面化):

- **症状**: `async fn getR(): Result<Box, string> { Result.ok(new Box(5)) }` を注釈なしで書くと **「cannot infer the type parameter(s) of \`Result\`」** (第 63 以前は Type::Any クラッシュ)。 `let r: Result<Box,string> = ...` と内側注釈すれば回避できていた。 非 async の generic メソッドに `Result.err(..)` を渡す形でも同症状。
- **原因**: async desugar ([gen_items.rs](../crates/ilang-parser/src/normalize/state_machine/gen_items.rs)) は async fn の return 値を `Promise.$promise.settleResolve(state_ref.__async_promise, Result.ok(..))` に包む。 `settleResolve<T>(p: Promise<T>, v: T)` は generic static method で、 promise (`__async_promise: Promise<Result<Box,string>>`) から T=Result<Box,string> が推論され、 v は本来 refine されるべき。 だが **`resolve_method_call` ([method.rs](../crates/ilang-types/src/checker/method.rs)) の generic メソッド経路が引数を `value_assignable` で検証するだけで `refine_enum_ctor_args` を呼んでいなかった** (generic fn 経路 [calls.rs] / `check_args` は呼ぶ)。 そのため `Result.ok(..)` の E=Any が残った。 この refine 漏れは settleResolve に限らず**全 generic メソッド呼び出し**に存在した一般バグ。
- **修正**: `resolve_method_call` の generic メソッド検証ループで、 各引数を subst 後の param 型に対して `refine_enum_ctor_args(arg, &actual)` (generic fn 経路と同形)。
- **検証**: async fn が `Result<Box,string>` を返し driver で ok/err を逐次 await。 値 (total=300) + deinit 厳密 (50、 ok 50 個が await 跨ぎで全解放・leak なし)。 fixture: `05_edge_cases/async_fn_returns_result.il`。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 **補足**: generic メソッドを T を決める引数なしで呼ぶ形 (`h.take<T>(Result.err(..))` で戻り型が T 無関係) は第 60/62 弾と同じ「T 決定不能」で診断対象 (注釈で回避)。

### [解決済み記録] 第 64 弾: enum-in-enum ctor の payload が refine されず Type::Any (2026-06-13)

第 63 弾の明示診断が表面化させた実バグ:

- **症状**: `let r: Result<Maybe<Box>, string> = Result.ok(Maybe.nope)` や `m["k"] = Result.ok(Maybe.nope)` (m が `Map<_, Result<Maybe<Box>,string>>`) で、 内側 `Maybe.nope` の T が決まらず **「cannot infer the type parameter(s) of \`Maybe\`」** (第 63 以前は Type::Any クラッシュ)。 宣言型 `Result<Maybe<Box>,_>` から内側の Maybe は Box と確定できるのに refine されていなかった。
- **原因**: `refine_enum_ctor_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) の EnumCtor arm は ctor の**自分の型引数**を target から埋めるだけで、 **payload 引数 (`Maybe.nope`) へ再帰していなかった**。 さらに ctor の自分の slot も「bare `Any` のみ置換」だったため、 引数から推論された `Maybe<Any>` (ネスト Any) が直らなかった (check_enum_ctor は Result の T を Maybe.nope の型 = Maybe<Any> と推論する)。
- **修正**: EnumCtor arm を 2 点拡張。 (1) `type_contains_any` ヘルパーで slot が Any を**ネスト含む**なら target の具体 arg で置換 (`Maybe<Any>` → `Maybe<Box>`)。 (2) enum sig から variant の payload 型を引き、 ctor の (置換後) 型引数で subst して **各 payload 引数を再帰 refine** (`Maybe.nope` を `Maybe<Box>` に対して refine → 内側 stash が Box に埋まる)。 既存の some / tuple / array 再帰と合わせ、 任意の深さの enum-in-enum / enum-in-some/tuple/array が refine される。
- **検証**: let 注釈・fn 戻り・map 値 (index store + 上書き)・二重入れ子 (`Result<Maybe<Maybe<Box>>>`) を heap payload 込みで網羅。 値の正しさ + deinit 厳密 (200/round) + churn delta=0。 fixture: `05_edge_cases/nested_enum_ctor_refine.il`。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 63 弾: 解決できない型引数を明示診断にした (2026-06-13、 ユーザー決定)

第 62 弾の残る限界 (期待型が皆無の bare match scrutinee 等) を、 ユーザー判断で**明示診断**にした (双方向推論は入れず、 曖昧なら注釈を促す):

- **背景**: `match f(Result.err("e")) { .. }` (期待型の無い bare scrutinee) や `let r = Result.ok(5)` (E が未決定) は、 型引数が `Any` のまま monomorphizer に届き **「mir lower: unsupported in M1: Type::Any (variadic builtins)」** で停止していた (源コード位置の無い不親切なエラー)。
- **調査**: checker は型引数を `enum_ctor_type_args` / `fn_call_type_args` の stash (span → (name, [type args])) に記録し、 解決できなかった param は `Type::Any` を残す。 到達コードではこの Any が lowering でクラッシュするが、 **未呼び出し fn 内の Any はコンパイルが通っていた** (テンプレートが lowering 前に破棄/未 monomorphize のため)。
- **修正**: `check()` ([check.rs](../crates/ilang-types/src/checker/check.rs)) の末尾に `report_unresolved_type_args` を追加。 両 stash を走査し、 型引数に `Type::Any` を含むエントリを `Unsupported { what, span }` で報告: enum ctor は「cannot infer the type parameter(s) of \`Result\` here — add a type annotation」、 generic fn 呼び出しは「cannot infer the type parameter(s) of generic fn \`f\` at this call」。 **`TypeVar` は除外** (generic テンプレート本体は instantiation で解決されるため正当)。 全ての refine (let / return / 引数 / match arm join / 第 54〜62 弾) が走った**後**の最終 stash を見るので、 解決済みの呼び出しは Any を持たず誤検知しない。
- **挙動変更**: 従来コンパイルが通っていた **未呼び出し fn 内の曖昧 ctor** (`fn unused() { let r = Result.ok(5); .. }`) も明示エラーになる。 これは到達性に関わらず曖昧 (Rust の `let r = Ok(5)` が注釈必須なのと同じ) なので妥当な厳格化。 既存 fixture / cocoa / std はいずれも曖昧 ctor に依存しておらず全緑。
- **検証**: 失敗していた全形 (bare match scrutinee・注釈なし let・`Result.ok(5)`) が span 付き診断に変わり、 正しい形 (注釈あり・T-fixing 引数・第 59/61/62 弾の各位置) は無影響。 fixture: `05_edge_cases/unresolved_type_arg_diagnostic.il` (`// expect-error` で診断を pin)。 workspace nextest 539/539 (REPL 含む)、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 **残る既知の課題**: 双方向推論 (match arm の使われ方から scrutinee の T を逆算) は未実装 — 曖昧なら注釈で回避する方針。

### [解決済み記録] 第 62 弾: 期待型から generic 呼び出しの T を解いた後に引数も再 refine (2026-06-13)

第 60 弾の確認済み記録 (2) を大部分解消:

- **症状**: `fn f<T>(r: Result<T,string>): Result<T,string>` を `f(Result.err("e"))` (T を決める引数なし) で呼び、 結果に**期待型がある**形 — `let x: Result<i64,string> = f(Result.err("e"))`、 fn 戻り位置、 引数位置 — でも **Type::Any** で lower 失敗。
- **原因**: 第 59 弾の `refine_fn_call_type_args` は fn の T を期待型から解いて呼び出しの stash を更新するが、 **インライン引数 `Result.err("e")` 自身の `enum_ctor_type_args` stash は `[Any, string]` のまま**だった。 引数は check 時にまだ generic な param 型 (`Result<Any,string>`、 inferred_args が Any) に対して refine されたため。 fn の T が後から i64 に解けても引数の stash は更新されず、 引数の lowering が Type::Any。
- **修正**: `refine_fn_call_type_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) で T を解いた後、 **各引数を「解いた型引数で具体化した param 型」に対して `refine_enum_ctor_args` で再 refine** する処理を追加 (引数は fn の T を共有する)。 これで `Result.err("e")` の T も i64 に埋まる。 borrow 衝突回避のため sig.params / type_params を clone し tbl を drop してから実施。
- **検証**: let 注釈・fn 戻り・引数位置で `f(Result.err(..))` / `f(Result.ok(Box))` を heap T 込みで網羅。 値の正しさ + deinit 厳密 (300/round = 3/round × 100) + churn delta=0。 fixture: `05_edge_cases/generic_fn_call_solved_from_context.il`。 **core 推論変更**のため workspace nextest 539/539 で回帰なし確認。 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 **残る限界**: 期待型が皆無の bare match scrutinee は未対応 (上の確認済み記録)。

### [解決済み記録] 第 61 弾: match の arm が yield する enum ctor が refine されず Type::Any (2026-06-13)

第 60 弾の確認済み記録 (1) を解消:

- **症状**: `let res = match r { ok(v) { Result.ok(v) } err(e) { Result.err(e) } }` の後 `res` を返す/使う形で **「mir lower: unsupported in M1: Type::Any (variadic builtins)」**。 **非 generic でも再現** (`r: Result<i64,string>` 固定でも失敗)。 primitive scrutinee (`match flag { 0 { Result.ok(..) } _ { Result.err(..) } }`) も同様。
- **原因**: match の各 arm が enum ctor を yield するとき、 `Result.ok(v)` は T のみ・`Result.err(e)` は E のみ pin し、 自分では片方を Any のまま残す。 match の結果型は arm の join で `Result<i64,string>` と正しく出るが、 その型は各 arm ctor の stash (`enum_ctor_type_args`) に **push back されない**。 let / return / 引数位置の refine は「値そのもの」を対象にするが、 ここでは値が **兄弟 arm によって補完される** ため、 注釈なし let に束縛すると refine の入口が無かった。
- **修正**: `refine_match_arm_ctors(arms, result_ty)` ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) を新設し、 join 後の結果型 (Generic / Optional のときのみ) を各 arm body に `refine_enum_ctor_args` で push。 match の 3 種の検査経路すべてに適用: enum scrutinee (`check_match_expr`)・Optional scrutinee (`check_match_optional`)・primitive scrutinee (`check_match_primitive` [match_.rs](../crates/ilang-types/src/checker/match_.rs))。 arm body が Block でも `refine_enum_ctor_args` が tail を辿る。
- **検証**: enum scrutinee の rewrap (heap `Box`)・generic 版 (`rewrapG<T>`)・primitive scrutinee の classify を網羅。 値の正しさ + deinit 厳密 (200/round = 2/round × 100) + churn delta=0。 fixture: `05_edge_cases/match_arm_yields_enum_refine.il`。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 60 弾: generic fn の引数型推論が Any を具体型より優先 (2026-06-13)

`?` を generic fn 内で使う形を probe して発見 (当初 `?` 起因に見えたが `?` 非依存):

- **症状**: `fn unwrapOr<T>(r: Result<T,string>, fallback: T): Result<T,string> { let v = r?; Result.ok(v) }` を `unwrapOr(Result.err("boom"), 0)` で呼ぶと **「mir lower: unsupported in M1: Type::Any (variadic builtins)」**。 `Result.ok(5)` の呼びは通る。 `?` 無しの `fn pick<T>(a: Result<T,string>, b: T): T { b }` を `pick(Result.err("e"), 42)` で呼んでも同症状。
- **原因**: generic fn 呼び出しの型引数推論 ([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs)) が各引数から `collect_type_var_bindings` で bind を集めるが、 同関数 ([sigs.rs](../crates/ilang-types/src/checker/sigs.rs)) が `bindings.entry(name).or_insert_with(..)` で **最初の binding を優先**。 arg1 `Result.err("boom")` は `Result<T,string>` vs `Result<Any,string>` で **T=Any** を入れ、 arg2 `0`/`42` の `T` vs `i64` が **上書きされない**。 結果 fn が `<Any>` で具体化され、 monomorphizer が `Result<Any,string>` の layout で `ty_to_mir(Any)` に達して停止。 (切り分け: DBG で fn の specialize が T=Any と T=i64 の両方で走っていることを確認。 非 generic の同形 (`Result<i64,string>` 固定) は通るので generic 推論固有。)
- **修正**: `collect_type_var_bindings` の `TypeVar` arm で、 **既存 binding が `Any` なら具体型で上書き** (具体同士は従来どおり最初を保持し、 conflict を黙って再推論しない)。 これで arg2 の i64 が arg1 の Any に勝ち、 fn が `<i64>` で具体化される。
- **検証**: `pick(Result.err("e"), 42)` / `pick(Result.ok(7), 9)` の値、 `?` helper の ok/err 経路を heap (`Box`) payload で deinit 厳密 (300/round = 3/round × 100) + churn delta=0 (err 経路の未使用 fallback Box 解放も確認)。 fixture: `05_edge_cases/generic_fn_arg_infer_prefers_concrete.il`。 **core 推論変更**のため workspace nextest 539/539 で回帰なしを確認。 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 59 弾: generic fn の型パラメータを戻り値位置から推論 (2026-06-13)

第 57 弾の判断待ち記録 (2) を解消 (これで判断待ち 2 件とも解決):

- **症状**: 型パラメータが戻り型にしか現れない generic fn は引数から T を決められず、 期待型から解かれなかった。 `fn makeArr<T>(): T[] { [] }` に `let xs: i64[] = makeArr()` で「expected i64[], got any[]」、 `fn makeNope<T>(): Maybe<T>` / `fn wrapErr<T>(): Result<T,string>` は (value_assignable は通るが) monomorphizer で Type::Any。 配列・user enum・builtin Result すべてで再現。
- **原因**: generic fn 呼び出しの型推論 ([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs) `check_call_expr`) は **引数からのみ** 型パラメータを bind し、 残りを `Type::Any` にして stash + 戻り型に subst する。 期待型 (let 注釈等) を使う経路が無かった。
- **修正**: `refine_fn_call_type_args(call_expr, target)` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) を新設。 generic fn 呼び出しの stash に Any が残るとき、 引数で既に決まった bind を seed し、 宣言戻り型 `sig.ret` を target に `collect_type_var_bindings` で unify して残りを解き、 `fn_call_type_args` を更新して補正後の戻り型を返す (enum-ctor の `refine_enum_ctor_args` と同じ post-hoc 方式)。 期待型が分かる 3 位置で適用: **let 注釈** ([stmt.rs](../crates/ilang-types/src/checker/stmt.rs)、 vt を補正)・**fn 戻り位置** ([decls.rs](../crates/ilang-types/src/checker/decls.rs)、 tail を expected に対して解き body_ty を補正)・**呼び出し引数** ([method.rs](../crates/ilang-types/src/checker/method.rs) `check_args`、 at を param 型で補正)。 引数で T が既に決まる呼び出し (`wrapOk(5)`) は stash に Any が無いので無影響。
- **検証**: let (配列 / Maybe / Result)・return 位置・引数位置・**部分推論** (`pair<A,B>(a: A): (A,B)[]` で A=引数・B=注釈)・heap T (`emptyBoxes<T>(): T[]` に Box) を網羅。 値の正しさ + deinit 厳密 (200/round = 2/round × 100) + churn delta=0。 fixture: `05_edge_cases/generic_fn_return_type_inference.il`。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 58 弾: generic class メソッドが generic enum を構築すると「unknown enum 〜」 (2026-06-13)

第 57 弾の判断待ち記録 (1) を解消:

- **症状**: `class Wrap<T> { v: T; asSome(): Maybe<T> { Maybe.some(this.v) }; asOk(): Result<T,string> { Result.ok(this.v) } }` を `Wrap<i64>` で使うと **「mir lower: unknown enum Maybe」 / 「unknown enum Result」**。 user enum も builtin Result も同症状。
- **原因**: class 単一化 ([class.rs](../crates/ilang-mir/src/monomorphize/class.rs)) の `subst_expr` は specialized method body の式の**型**を T→具体に置換するが、 **enum ctor の `enum_name` を再 mangle しなかった**。 そのため `Maybe.some(this.v)` の `enum_name="Maybe"` がそのまま lower に届き、 resolve_ty が bare `Type::Enum("Maybe")` を引けず失敗。 generic fn 経路 ([fns.rs](../crates/ilang-mir/src/monomorphize/fns.rs)) は `rewrite_calls_and_enums_in_expr` で enum_ctor_type_args + outer 置換から再 mangle するが、 class 経路に同等が無かった (span 記録の型引数は generic で [TypeVar(T), ..]、 specialize の clone は span を共有するので、 置換は specialize 時にしかできない)。
- **修正**: checker の `enum_ctor_type_args` を **thread-local** (`ENUM_CTOR_TYPE_ARGS` [mod.rs](../crates/ilang-mir/src/monomorphize/mod.rs)、 既存の `GENERIC_ENUM_NAMES` と同パターン) 経由で class pass に渡す。 `monomorphize` / `monomorphize_with_requests` に `enum_ctor_type_args` 引数を追加し、 pass 開始時に thread-local へ stash。 `subst_expr` が EnumCtor を見たら `remangle_generic_enum_ctor` で span 記録の型引数を class の params→args で置換し、 concrete なら `mangle_enum` で `Maybe<i64>` / `Result<i64,string>` に (fn 経路と同形)。 ctor 引数は再帰 subst で内側 ctor も処理 (`Result.ok(Maybe.some(this.v))`)。 非 generic class は `subst_expr` を通らない (rewrite_item 経由) ので無影響。 caller 3 箇所 (main.rs 通常 2 + REPL 1) に `tc.enum_ctor_type_args()` を渡す。
- **検証**: builtin Result・user enum Maybe を generic class メソッドで構築 (heap T `Box` 込み)、 複数インスタンス化 (`Wrap<i64>` / `Wrap<string>` / `Wrap<Box>`)、 入れ子 (`Result<Maybe<T>, string>`)、 explicit pattern での match 消費を網羅。 値の正しさ + deinit 厳密 (200/round = 2/round × 100) + churn delta=0。 fixture: `05_edge_cases/generic_class_method_builds_enum.il`。 **別の既存制限を記録**: generic class の **static メンバ**は checker が「static members on generic classes are not supported」で拒否 (本バグと無関係の意図的制限)。 monomorphize pipeline を触ったので nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 57 弾: generic fn が builtin Result を構築すると「unknown enum Result」 (2026-06-13)

generic fn × enum ctor 型引数精緻化 (第 48〜56 弾) の継ぎ目を probe して発見:

- **症状**: `fn wrapOk<T>(x: T): Result<T, string> { Result.ok(x) }` を `wrapOk(5)` で呼ぶと **「mir lower: unknown enum Result」**。 `Result.err(e)` 構築も同様。 user 宣言の generic enum (`fn wrapM<T>(x: T): Maybe<T> { Maybe.some(x) }`) は動く。
- **原因**: builtin `Result<T,E>` は宣言が無く call site ごとに monomorphize される (`monomorphize_enums` が `Result<i64,string>` 等の `Item::Enum` を合成)。 `monomorphize_fns` ([fns.rs](../crates/ilang-mir/src/monomorphize/fns.rs)) は generic fn を specialize する際、 body 内の generic-enum `EnumCtor.enum_name` を具体形 (`Result.ok` → `Result<i64,string>.ok`) に再 mangle するが、 「どの enum 名が generic か」の集合 `generic_enums` を**プログラム宣言の `Item::Enum` だけ**から作っていた。 builtin Result はそこに無いため mangle されず、 specialized body の `Result.ok` が `enum_name="Result"` のまま lower に届き、 resolve_ty が bare `Type::Enum("Result")` を引けず失敗。
- **修正**: `monomorphize_fns` の `generic_enums` 構築後に `result_template()` ([enums.rs](../crates/ilang-mir/src/monomorphize/enums.rs)、 `pub(super)`) を seed (`monomorphize_enums` が line 66 で行うのと同じ対処)。 これで specialized body の Result ctor が enum_table (enum_ctor_type_args) + outer 置換から `Result<i64,string>` に mangle される。
- **検証**: T を ok arg / err arg / 両腕で決める形を i64 と heap (`Box`) payload で網羅し、 値の正しさ + deinit 厳密 (300/round = 3/round × 100) + churn delta=0 (err 腕が破棄する heap arg の解放も確認)。 fixture: `05_edge_cases/generic_fn_returns_result.il`。 monomorphize (lowering pipeline) を触ったので nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 56 弾: tail 式の奥に入れ子の `?` の err return が refine されず Type::Any (2026-06-13)

`?` 演算子 × enum ctor 型引数精緻化 (第 48〜54 弾) の継ぎ目を probe して発見:

- **症状**: `Result.ok(take(g(ok)?))` のように `?` が **tail 式の奥** (call 引数 / `some(..)` / tuple 要素 / array 要素 / 二重ネスト call) に埋まると **「mir lower: unsupported in M1: Type::Any (variadic builtins)」**。 `let b = g(ok)?` のように `?` が**自前の文**にあるときや、 `let b = g()?; Result.ok(b.n)` は元から動いていた。
- **原因**: `?` は [postfix.rs](../crates/ilang-parser/src/expr/postfix.rs) で `{ let __try = expr; match __try { ok(v) { v } err(e) { return Result.err(e) } } }` に desugar される。 この `return Result.err(e)` は E (=e の string) は埋まるが **T=Any** で、 enclosing fn の戻り型から refine される必要がある。 `refine_enum_ctor_args_in_block` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) は **block の文**の値式には `refine_returns` (全子孫を歩いて Return を refine) を呼ぶのに、 **tail** には自身の値 ctor を refine する `refine_enum_ctor_args` しか呼んでいなかった。 そのため tail 式の奥に埋まった `return Result.err` が見つからず T=Any のまま lower に届いていた。
- **修正**: tail にも `refine_returns(self, t, target)` を適用 ([utils.rs](../crates/ilang-types/src/checker/utils.rs))。 `refine_returns` は式の全子孫を再帰的に歩くので、 call 引数 / some / tuple / array / 二重ネストのどの深さに `?` があってもその err return が refine される。 tail 自身の値 ctor refine (既存の `refine_enum_ctor_args`) と相補的 (前者は Return ノードのみ、 後者は tail 値)。
- **検証**: `?` を call 引数 / some / tuple / array / 二重ネスト call に入れ子にした 4 形を、 ok 経路 (heap payload を `?` が unwrap → 解放) と err 経路 (入れ子位置から早期 return) の両方で deinit 厳密 (400/round = 4/round × 100) + churn delta=0。 値の正しさ (21) も確認。 fixture: `05_edge_cases/try_nested_in_call_arg_refine.il`。 checker のみの変更のため nested_generic 儀式は対象外。 workspace nextest 539/539、 AOT 全 fixture PASS。

### [解決済み記録] 第 55 弾: 空マップリテラル `{}` を型注釈ありで空マップと解釈 (2026-06-13、 ユーザー決定)

第 54 弾で記録した「`let m: Map<K,V> = {}` が型エラー」を、 ユーザー判断 (= `{}` でも書けるように実装) を受けて対応。 **`new Map<K,V>()` は従来から空マップを作れる確立した形** (第 54 弾の当初記録の「構文が無い」は誤りで訂正済み)。 本弾は JS 風に `{}` でも書ける利便性の追加:

- **症状 (= 第 54 弾の別件)**: `{}` はパーサが**空ブロック** (値 `()`) として出すため (`control.rs::parse_map_literal` は 1 エントリ以上のときだけ `MapLit` を出す)、 `let m: Map<K,V> = {}` が「expected Map<K,V>, got ()」。
- **対応**: 型情報の要る checker + lowering で「Map ターゲット位置の空ブロック = 空マップ」とした。 (1) checker `value_assignable` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) が **空ブロック (stmts 空 + tail なし) を `Map<K,V>` ターゲットへ受理** (`empty_block_as_map`)。 非空 `{k: v}` は元から `MapLit` なので無関係、 空ブロックの unit 用法は Map 以外のターゲットでは不変。 (2) lowering `lower_composite_with_hint` ([literals.rs](../crates/ilang-mir/src/lower/literals.rs)) が `(Block 空, MirTy::Map)` を **空 `NewMap`** に lower (`lower_map_literal_with_hint(&[], ..)`)。 (3) `is_fresh_object_expr` ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs)) が**空ブロックを fresh 扱い** — 空ブロックは map スロットで rc=1 の fresh `NewMap` になり、 束縛が scope-exit で解放する必要があるため。 unit 文脈では fresh フラグは arc_heap でないので no-op (安全)。
- **検証**: let 注釈 / 再代入 (`m = {}` で旧マップ解放) / field 代入 (`this.m = {}`) / fn 戻り値 / fn 引数 / ネスト map 値 (`m.set("o", {})`) の全位置で heap value 込み deinit 厳密 (600/round = 6/round × 100) + churn delta=0。 空ブロックの unit 用法 (`if c {}`、 `let x = {}`) の回帰なし。 fixture: `03_collections/empty_map_literal_brace.il`。 checker + lowering 両方を触ったので nested_generic 儀式も実施。 workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 54 弾: enum ctor 型引数精緻化を payload 引数 / built-in 配列メソッド / index 代入へ拡張 (2026-06-13)

第 48〜50 弾で enum ctor 型引数精緻化を let / field / 配列・Map リテラル / fn・method・init 引数 / some・tuple / fn・closure 戻り値へ広げたが、 同じ「型パラメータを引数から埋められない enum ctor」が **3 系統の残る store 位置**で Type::Any のまま lower に届いていた:

- **症状**: `Result<_, string>` 等 (T=Any) を以下に置くと **「mir lower: unsupported in M1: Type::Any (variadic builtins)」** (到達可能 = monomorphize される時)。 (1) **enum payload 引数**: `Wrapper.wrap(Result.err("e"))` (tuple payload) / `WrapperS.wrapS { r: Result.err("e") }` (struct payload)。 (2) **built-in 配列メソッド引数**: `xs.push(Result.err("e"))`・unshift・fill・remove・indexOf・includes。 (3) **index 代入**: `xs[0] = Result.err("e")` (配列) / `m["k"] = Result.err("e")` (Map)。 `Result.ok(1)` 等 T が埋まる形・到達不能な未呼び出し fn は元から通っていた。
- **原因**: refine は「値が宣言型に出会う store 位置」ごとに明示的に呼ぶ設計だが、 上記 3 経路が呼び漏れていた。 (1) `check_enum_ctor` ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) は引数を payload 型へ `value_assignable` で検証するのに `refine_enum_ctor_args` を呼んでいなかった。 (2) ハードコードされた配列メソッド ([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs) の `Type::Array` arm) は element 型へ検証するのに refine 無し (Map.set / Set.add は builtin クラス → `check_args` 経由で第 49 弾の refine が効いて健全)。 (3) `check_assign_index` ([access.rs](../crates/ilang-types/src/checker/expr/access.rs)) の Map value / 配列 element 検証に refine 無し。
- **修正**: 3 ファイルに `refine_enum_ctor_args(arg, &target)` を追加。 `check_enum_ctor` は tuple / struct 両 payload の検証ループで各引数を payload 型へ refine (各 ctor が自分の引数を refine するので **ネストした ctor-in-ctor も自然に伝播**)。 配列メソッド 6 種 (push/unshift/fill/remove/indexOf/includes) は element 型へ。 `check_assign_index` は Map value 型と配列 element 型へ。
- **検証**: payload (tuple/struct)・push・index 代入 (配列/Map) に `Result<Box, string>` を heap payload 込みで格納し deinit 厳密 (600/round = 6/round × 100) + churn delta=0。 ok/err 混在・値の正しさ (5/6/3/-1/9) を確認。 fixture: `05_edge_cases/enum_ctor_type_arg_refine_payload_and_index.il`。 **これで enum ctor 型引数精緻化は全 store / 引数位置で揃った。** workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS。 checker のみの変更のため nested_generic 儀式は対象外。
- **別件の既存ギャップを記録** (バグでなく設計判断待ち): 空マップリテラル `{}` は **パーサが空ブロックとして出す**ため (`casts.rs:484` のコメント「parser only ever emits MapLit when there's at least one entry」)、 `let m: Map<K,V> = {}` が「expected Map<K,V>, got ()」で型エラー。 空マップを作る構文が無い。 対応するなら空マップ構文の決定 (`{}` を注釈ありなら空マップと解釈する / `{:}` 等の専用構文 / `Map()` コンストラクタ) が要り、 `{}` (空ブロック) との曖昧性解決を含む言語仕様の選択。

### [確認済み記録] 第 53 弾: slot 昇格 edge + Map forEach mutation — 全て健全 (2026-06-13)

第 52 弾 (accessor の slot 昇格) の周辺と、 Map forEach の反復中 mutation を probe。 **新規バグなし**:

- **slot 昇格の他経路**: メソッド内クロージャ (`makeReader(): fn(){ g[0] }`)・accessor 内クロージャ (`get reader(): fn(){ g[0] }`)・init (`init() { g[0] = g[0]+1 }`)・static メソッド内クロージャ・default 引数式 (`fn f(x = g+1)`) からのグローバル参照が全て host slot に昇格され動作。 `walk_block` がクロージャ本体へ再帰し、 第 52 弾の accessor 走査と合わせて漏れなし。
- **Map.forEach の反復中 mutation**: forEach は callback ループ開始時に entry 順を **snapshot** するため安全。 (1) **add-during**: 反復中に `m.set(..)` した値は map drop 時に解放、 当該反復では未訪問。 (2) **future-key delete**: 反復中に未訪問キーを delete しても snapshot の +1 が callback 終了まで値を生存させ **UAF なし**・なお訪問される (sum に含まれる)。 (3) **nested forEach** (同一 map 二重反復): 二重計上も leak も無し。 全形 deinit 厳密 (800/round) + churn delta=0。

fixture: `03_collections/map_foreach_mutation_arc.il` (add/future-delete/nested を厳密 deinit + churn delta=0)。 slot 昇格 edge は第 52 弾 fixture と本ラウンドの probe で確認済みのため fixture 追加なし。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 52 弾: property accessor body から top-level let を参照すると unbound (2026-06-13)

property accessor を probe して発見:

- **症状**: property getter / setter の本体から **top-level let (グローバル)** を読み書きすると **「mir lower: unbound variable: g」**。 同じ body を **regular method** に書けば動く (`deinit() { deinits[0] = .. }` が全 probe で効いていたのと同じ機構)。 getter/setter だけが参照する let で発生。
- **原因**: CLI の slot 昇格パス `build_slot_table` ([main.rs](../crates/ilang-cli/src/main.rs)) が `collect_fn_free_var_refs` ([walk.rs](../crates/ilang-cli/src/walk.rs)) で「fn / method body から参照される top-level let」を集めて host slot に昇格するが、 `walk_class` が `c.methods` / `c.static_methods` は走査するのに **`c.properties` (getter/setter) を走査していなかった**。 結果、 getter/setter からしか参照されない let は `slot_table` → `repl_slots` に入らず、 lowering の `lower_var_expr` が repl_slots でも解決できず unbound。 debug で getter を持つプログラムの `repl_slots` が空 (=昇格漏れ) と確認。
- **修正**: `walk_class` に `c.properties` の getter/setter body 走査を追加 (`this` + params を locals に、 method と同形)。 これで accessor 内のグローバル参照が昇格される。
- **検証**: getter がグローバル配列を read / write (counter インクリメント)、 setter がグローバルを参照、 getter の副作用が 1 アクセス 1 評価 (getCalls=3)・setter 呼び出し回数 (setCalls=2) を確認。 scalar グローバルも (昇格漏れで repl_slots 空だったのが) 解消。 fixture: `08_properties/property_accessor_reads_global.il`。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 51 弾: heap static フィールド代入が retain せず use-after-free (2026-06-13)

static フィールドを probe して発見した ARC バグ:

- **症状**: `Cls.s = arg` (s が string static フィールド、 arg が fresh string を持つ借用 param) の後 `Cls.s` を読むと **空文字を印字するのに length は正しい** = 解放済みバッファを参照する **UAF**。 fresh string を **直接** (`Cls.s = "a" + b`) なら動く (fresh が +1 を所有)、 リテラルや束縛 local も動く (長生き)。 fresh を**パラメータ経由**で渡したときだけ壊れる。
- **原因**: static フィールド代入の lowering ([expr.rs](../crates/ilang-mir/src/lower/expr.rs) の `StoreStatic` 経路) が **retain も release もしていなかった**。 instance field (`StoreField`) は retain-new / release-old するのに、 static slot は値をそのまま store。 借用源は slot が +1 を所有しないため、 源 (caller の fresh arg transient) が解放されると slot が dangling。
- **修正**: heap static slot (`is_arc_slot`) のとき instance field と同じ **retain-new (非 fresh) / release-old (init store は除く — const-baked 初期値を持つため)** を `StoreStatic` の前に追加。 fresh 源は +1 を slot の share に流用、 借用源は retain。 旧値 (前回の文字列 or const-init "init") を release (interned リテラルの release は安全と churn で確認)。
- **検証**: fresh 直接・fresh via param (旧 UAF)・借用 local を string static に代入し、 値の非破壊 (`hello` が `hello` のまま) と `liveStringCount` churn 600 で flat (retain/release 均衡)・read が生きたバッファを返すことを確認。 fixture: `02_classes/static_string_field_arc.il`。
- **別の既存制限を記録** (バグでなく codegen ギャップ): **数値配列の static フィールド** (`static data: i64[]`) は checker が許可するのに **codegen が「unsupported in M1: static slot type」で拒否**。 string static のみ実用。 対応するなら static slot codegen に array 型を追加。
- workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 50 弾: クロージャ戻り値の enum ctor 型引数を精緻化 (2026-06-13)

第 48/49 弾で enum ctor 型引数精緻化を store / 引数 / some / tuple へ広げたが、 **クロージャ本体の戻り値**だけ未配線だった:

- **症状**: `let f: fn(): Result<i64,string> = fn(): Result<i64,string> { Result.err("e") }` や `xs.map(fn(x): Result<...> { ... Result.err(..) })` が **「Type::Any (variadic builtins)」**。 早期 return (`fn() { if .. { return Result.err(..) } .. }`) も同様。 top-level fn `fn f(): Result<..> { Result.err(..) }` は decls.rs:637 で refine 済みなので動いていた。
- **原因**: top-level fn 本体は [decls.rs](../crates/ilang-types/src/checker/decls.rs) の `refine_enum_ctor_args_in_block(&f.body, &expected)` で戻り値型から refine するが、 **クロージャ (`check_fn_expr` [casts.rs](../crates/ilang-types/src/checker/expr/casts.rs)) は同等の refine を呼んでいなかった**。 クロージャ本体の tail / return の `Result.err(..)` が T=Any のまま lower に届いた。
- **修正**: `check_fn_expr` の body 検査後に `self.refine_enum_ctor_args_in_block(body, &expected)` を追加 (`expected` = クロージャの宣言戻り値型)。 `refine_enum_ctor_args_in_block` は tail と全 return を走査するので tail / 早期 return の両方をカバー。
- **検証**: クロージャ tail err・早期 return err・`map` コールバックが Result を返す形を heap payload 込みで deinit 厳密 (300/round) + churn delta=0。 第 48/49 弾と既存 fixture の回帰なし。 fixture: `10_closures_arc/enum_ctor_refine_closure_return.il`。 **これで enum ctor 型引数精緻化は let / field (明示+bare) / 配列 / Map / 再代入 / 引数 (fn/method/init/default) / some / tuple / 入れ子 / top-level fn 戻り値 / クロージャ戻り値の全位置で揃った。** workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 49 弾: enum ctor 型引数精緻化を引数 / some / tuple 位置へ拡張 (2026-06-13)

第 48 弾で field / 配列 / Map / 再代入を直したが、 同じ「型パラメータを埋めない enum ctor」が**残りの位置**でも Type::Any で失敗していた:

- **症状**: `consume(Result.err("e"))` (fn 引数)・`s.handle(Result.err("e"))` (method 引数)・`new Svc(Result.err(..))` (init 引数)・`some(Result.err(..))`・`(Result.err(..), 5)` (tuple 要素) が **「Type::Any (variadic builtins)」**。 入れ子 (`some((Result.err(..), 5))`、 `[(Result.ok(1), 10), (Result.err("e"), 20)]`) も同様。
- **原因**: 第 48 弾の `refine_enum_ctor_args` は if/match/block/return には再帰したが **`some` / tuple / array リテラルには再帰しなかった**ため、 `some(ctor)` の内側や tuple 要素の ctor が精緻化されなかった。 さらに **call-arg checker** (非 generic fn の `check_args` [method.rs](../crates/ilang-types/src/checker/method.rs)、 generic fn の inline arg loop、 fn 型 call) は refine を一切呼んでいなかった。
- **修正**: (1) `refine_enum_ctor_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) に `Some(inner)` (Optional の inner へ)・`Tuple(elems)` (各要素を tuple 型の対応要素へ)・`Array(elems)` (各要素を elem 型へ) の再帰 arm を追加 — これで let / field / 引数のどの入口から呼ばれても入れ子の ctor まで届く。 (2) call-arg の 3 経路 ([method.rs](../crates/ilang-types/src/checker/method.rs) の `check_args`、 [calls.rs](../crates/ilang-types/src/checker/expr/calls.rs) の generic fn arg loop と fn 型 call) で param 型を target に refine。
- **検証**: fn / method / init 引数・some・tuple 要素・入れ子 (some of tuple、 array of tuples) に Result を heap payload 込みで渡し deinit 厳密 (300/round) + churn delta=0。 第 48 弾 fixture と既存 fixture の回帰なし。 fixture: `05_edge_cases/enum_ctor_type_arg_refine_arg_some_tuple.il`。 workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 48 弾: enum ctor の型引数が store 位置で精緻化されず Type::Any (2026-06-13)

Result `?` を probe する過程で発見した型推論バグ:

- **症状**: `Result<i64, string>` や `Either<A, B>` (= **2 型パラメータ enum**) を **class field 型 / 配列要素型 / Map 値型**に使うと **「mir lower: unsupported in M1: Type::Any (variadic builtins)」**。 1 型パラメータ enum (`Maybe<T>`) は field/配列とも動く。 **local / 戻り値**はどちらも動く。 array リテラルでは `[Result.ok(1), Result.err("e")]` のように **`err`/`nope` 等パラメータを埋めない variant** を混ぜると 1 型パラメータでも失敗。
- **原因**: enum コンストラクタ `check_enum_ctor` ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) は引数から型パラメータを推論し、 **埋まらない param は `Any`** として span キーの `enum_ctor_type_args` に stash する (monomorphize がこれを読む)。 `Result.err("e")` は E しか埋まらず T=Any、 `Maybe.nope` は何も埋まらない。 `let f: T = ctor` と fn return は `refine_enum_ctor_args` で**注釈から Any slot を埋め直す**が、 **field 代入 (明示 `check_assign_field` / bare implicit-this)・配列リテラル要素 (`check_array_with_hint`)・Map 値 (`check_map_lit_with_hint`)・local 再代入**は refine を呼んでおらず Any が残ったまま lower に届いていた。
- **修正**: 5 箇所すべてで宣言型を target に `refine_enum_ctor_args(value, &decl_ty)` を呼ぶ — `check_assign_field` ([access.rs](../crates/ilang-types/src/checker/expr/access.rs))、 bare field 代入と local 再代入 ([expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs) の Assign arm 2 経路)、 `check_array_with_hint` 各要素、 `check_map_lit_with_hint` 各値 ([casts.rs](../crates/ilang-types/src/checker/expr/casts.rs))。 これで 2 型パラメータ enum を field/配列/Map に格納でき、 `err`/`nope` 混在配列も通る。
- **検証**: Result/Either を field (明示+bare init+再代入)・配列リテラル (ok+err 混在)・Map 値に heap payload 込みで格納し deinit 厳密 (400/round) + churn delta=0。 1 型パラメータ enum・local/return は回帰なし。 fixture: `05_edge_cases/enum_ctor_type_arg_refine_in_stores.il`。 workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 47 弾: interface covariance + downcast — 全て健全 (2026-06-13)

第 46 弾で interface 分岐 join を直した後、 interface の **covariance (実装クラスを親 interface スロットへ)** と **downcast** を網羅 probe。 **新規バグなし** — subclass 用の wrap/covariance/downcast 機構が interface にも generalize:

- **covariance**: `Circle → Shape?` (Optional wrap・bare field 代入)、 enum payload `shape: (Shape)` への Circle、 `Map<string, Shape>` への Circle/Square 混在リテラル、 `Circle[]/Square[] → Shape[]?` 入れ子コンテナ (第 28 の interface 版)、 `Map<string, Shape[]?>`、 generic `Holder<Shape>` への Circle、 `(Shape, i64)` tuple 要素、 `(Shape?, i64)[]` への tuple index store (slot0 wrap + interface)。 全て Optional/コンテナ越し dispatch で実体確認しつつ deinit 厳密。
- **downcast**: `as?` (NOT `as` — `as` は upcast/プリミティブ専用で downcast は型エラー)。 `sh as? Circle` で interface→具象に downcast、 成功は `some(circle)` で**共有** (二重解放なし)、 失敗 (別実装) は `none` (leak なし)。 クラス源 `a as? Dog` も同様。 `is T` の型テストは動くが**フロー絞り込みは無い** (`if a is Dog { a.fetch() }` は不可・`a as? Dog` で明示 downcast する仕様)。
- 計測の `delta=56` は §4-1 の `acc` 罠、 deinit は全形で厳密一致。

fixture: `09_subtyping/interface_covariance_and_downcast.il` (Optional/enum/Map/tuple/generic covariance + `as?` downcast を厳密 deinit 700 + churn delta=0)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 46 弾: 分岐 join が共通 interface に合流しない (2026-06-13、 ユーザー判断 = あるべき形に修正)

interface dispatch を probe して発見した型推論バグ:

- **症状**: `if c { new Circle() } else { new Square() }` のように **異なる interface 実装クラス**を返す `if`/`match`/配列リテラル/Map リテラルが、 両者が共通 interface `Shape` を実装していても **「type mismatch: expected Circle, got Square」で拒否**。 戻り値型 `Shape` や `let r: Shape = if..` の明示注釈があっても同じ。 **共通親クラスのサブクラス** (`Dog`/`Cat` → `Animal`) の同じ分岐は**動く**ので、 「クラス階層は join するが interface は join しない」非対称。
- **原因**: 分岐/リテラルの object-join が [utils.rs](../crates/ilang-types/src/checker/utils.rs) の `common_ancestor` (= クラス階層の共通祖先) しか見ておらず、 共通 interface を join 先候補にしていなかった。 該当箇所は `unify_branch_obj` (match)、 `check_if_expr` の class_join (typed if)、 `unify_optional_branches` の Object arm、 配列リテラル要素 join / Map 値 join ([casts.rs](../crates/ilang-types/src/checker/expr/casts.rs))、 非 typed if の join ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) の計 6 箇所。
- **修正 (ユーザー判断 = あるべき形)**: `common_object_join(a, b)` を新設 — まず `common_ancestor`、 無ければ **両クラスが実装する共通 interface**を探し、 **唯一なら**それを返す。 6 箇所すべてを `common_ancestor` から `common_object_join` へ切替。 これで if/match/配列/Map のどの分岐でも「親クラス or 唯一の共通 interface」へ合流。 **複数共通 interface**は join 先が曖昧 (この時点で期待型が無く一意に選べない) なので `None` を返し従来どおり型エラー — 注釈/構造変更で回避 (`branch_join_common_interface_ambiguous_error.il` で pin)。 共通点が無ければ従来どおりエラー (回帰なし)。
- **検証**: 唯一共通 interface の if/match/配列リテラル合流 + interface 経由 heap 返しメソッドの dispatch を deinit 厳密 (400/round) + delta=0。 複数共通 interface = clean な型エラー、 共通なし = 従来エラー。 multi-interface 実装クラス・interface 配列の for-in dispatch も健全。 fixture: `09_subtyping/branch_join_common_interface.il` (if/match/array 合流 + ARC)、 `09_subtyping/branch_join_common_interface_ambiguous_error.il` (複数 interface 曖昧)。 syntax.md / syntax_ja.md の interface 節に branch join を追記。 workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 45 弾: weak 参照 + 固定長配列 × wrap — 全て健全 (2026-06-13)

weak と固定長配列 (第 19 弾の rc 表現) を、 最近の wrap 修正と交差させて probe。 **新規バグなし**:

- **weak.get() の昇格**: strong 生存中は `some(値)`、 死後は `none` (dangling weak の .get() は crash せず死を検知)。
- **weak? back-ref サイクル** (child が parent を weak 参照): 両ノード解放・サイクル leak なし。
- **parent-owns-child (strong) + child-weak-parent**: parent 死亡カスケードが child を解放する最中に child の weak-parent 解放が走っても **二重解放しない** (d3b1d2cf の修正が保持)。
- **weak 配列** `Box.weak[]`: strong は所有者の scope で死に、 weak 配列 drop は strong を release しない (deinit 厳密)。
- **固定長配列 × 要素 wrap**: `Box?[2]` のリテラル (`[some(box), none]`) と index store (`arr[i] = box` の `Box → Box?`)、 `(Box?, Box)[2]` への tuple index store (slot0 wrap)。 第 36/41 の wrap 修正が **固定長配列の rc 表現にも generalize** していることを Optional 越し match で実体確認しつつ deinit 厳密 (700/round) + delta=0。

fixture: `05_edge_cases/fixed_array_optional_tuple_wrap.il` (固定長 Optional/tuple 要素 wrap を厳密 deinit 700 + churn delta=0)。 weak 系は既存 fixture (`arc_cycle_via_weak` / `weak_backref_cascade_release_order` / `leak_weak_*`) が pin 済みのため追加せず。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [確認済み記録] 第 44 弾: string / array ARC 全方位 — 第 43 弾以外は健全 (2026-06-13)

第 43 弾の inplace concat 修正の周辺と、 触っていなかった string/array 操作を網羅 probe。 **新規バグなし**:

- **string メソッド連鎖** `("v"+n).toUpper().slice(0,3)`: fresh 中間文字列が全て解放 (delta=0)。
- **template literal の heap 補間** `\`box ${b.toString()} val ${b.n}\``: 補間 temp と Box を解放 (string delta=0・box deinit 厳密)。
- **`+=` desugar**: `s += n.toString()` は `s = s + ...` に展開され第 43 弾の fix が効く (delta=0・値正しい)。
- **self-concat `s = s + s`**: inplace パターンに合致 (lhs Var==target) するが rhs は **借用** (`is_fresh_object_expr(Var)` = false) なので解放しない — rhs が lhs バッファをエイリアスし結果になるため正しい。 prepend `s = fresh + s` は inplace 非マッチで通常 concat 経路 (既存 fix)。 両者 delta=0。
- **array push / unshift** の fresh 要素、 **map** コールバックの fresh heap 返り、 **split** の string[]: 全て deinit 厳密・leak なし。
- **heap-kind 変数の fresh 再代入** (`m = mkMap(...)` / `arr = mkArr(...)`): 旧値解放・新 fresh 非 leak (deinit 厳密)。

fixture: `05_edge_cases/string_arc_chains_template_selfconcat.il` (メソッド連鎖・template heap 補間・`+=`・self-concat・split を churn 500 で `liveStringCount` flat + deinit 厳密)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 43 弾: inplace 文字列 concat が fresh rhs をリーク (2026-06-13)

tuple/generic を離れて string バッファ ARC を probe して発見:

- **症状**: `s = s + n.toString()` を回すと **1 文字列/反復のリニアな leak** (`liveStringCount` で N=100→100・200→200・400→400)。 `s = s + "-end"` (リテラル rhs) や `let s2 = s + n.toString()` (再代入なし) は無事 — leak は **inplace 再代入 × fresh rhs** に限定。
- **原因**: `s = s + <expr>` (s が string Local) は [expr.rs](../crates/ilang-mir/src/lower/expr.rs) の `Assign` arm で **`StrConcatInplace`** にルートされ、 `s` のバッファを doubling realloc で伸ばして rhs バイトを**追記**する。 rhs を**コピーするだけで消費しない**のに、 lowering が rhs を解放していなかった。 fresh rhs (`toString()` の temp・fresh concat) は +1 を持つので、 それが宙に浮いて毎反復 registry に溜まる。 リテラルは intern 済み (非所有)、 借用 var は所有者が解放するので、 どちらも fresh でなく無事だった。
- **修正**: `StrConcatInplace` の直後に **rhs が fresh なら Release** (`is_fresh_object_expr(rhs)` で判定、 op が rhs を読み終えた後)。 借用 rhs・リテラル rhs は非 fresh なので不変 (過剰解放しない)。
- **同族確認**: fresh rhs (toString / `(a + b)` の fresh concat) は churn 1000 で delta=0、 借用 rhs (`s = s + other` を 2 回) は値正しく (`aBBBB2`) churn delta=0 で過剰解放なし、 値の非破壊 (`v5-end`) を確認。 非再代入の `let s = a + b` は別パス (lower_binary StrConcat、 既存 fixture `leak_string_concat_loop.il` が pin 済み) で元から健全。
- fixture: `05_edge_cases/leak_string_concat_inplace_reassign.il` (fresh / 借用 rhs を churn 1000 で `liveStringCount` 増加 < 50)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 42 弾: tuple 要素 wrap の残る store サイト — 全て健全 (2026-06-13)

第 36 弾 (bare field) / 第 41 弾 (index store) で composite リテラルの要素 wrap を直した後、 **同じ `(Box?, Box)` 形を残りの全 store サイト**で攻めた。 **新規バグなし** — これらは元から `lower_composite_with_hint` / `lower_arg_to` 経由でヒントを受けており健全:

- **return 位置** `fn(): (Box?, Box) { (box, b) }`、 **`some(tuple)`** (`(Box?, Box)?` への some)、 **enum payload** (`variant: ((Box?, Box))`)、 **引数位置** `consume((box, b))`、 **local 再代入** `t = (box, b)`、 **明示 field 代入** `this.pair = (box, b)`: 全て slot0 を `Box → Box?` wrap して値正しく、 deinit 厳密・delta=0。
- **入れ子** `((Box?, Box), Box?)` (内側 slot0 + 外側 slot1 の二重 wrap): 値正しく ARC 均衡。
- **tuple 内 weak 要素** `(Box.weak, Box)`: slot0 の strong→weak coerce が strong を **retain で漏らさない** (weak ref が指す strong box は所有者の scope で死ぬ・deinit 2/round)。 weak への直接 `match` は第 34 弾どおり checker が拒否 (バグでなく既存制約)。

これで composite (tuple) 要素 wrap は **全 store サイト** (let / 引数 / return / 再代入 / field (bare + 明示) / index (array + map) / some / enum payload / 入れ子 / weak) で網羅確認済み。 fixture: `05_edge_cases/tuple_element_wrap_all_stores.il` (return/some/enum/arg/reassign/field/nested を厳密 deinit 1700 + churn delta=0)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 41 弾: index 代入の RHS が composite 要素 wrap を欠いて SIGSEGV (2026-06-13)

tuple をコンテナに入れる ARC を probe する過程で発見。 第 36 弾 (bare field 代入) と同じ「composite リテラルの要素 wrap 欠落」が、 **index 代入経路**に残っていた:

- **症状**: `arr[0] = (new Box(7), new Box(3))` (`arr: (Box?, Box)[]`) や `m["k"] = (...)` (`Map<_, (Box?, Box)>`) が **SIGSEGV**。 配列/Map の **リテラル**形 (`[(box, b)]` / `{"k": (box, b)}`) と **bare tuple** (`let t: (Box?, Box) = (box, b)`) は無事、 **index 代入だけ**落ちる。
- **原因**: [expr.rs](../crates/ilang-mir/src/lower/expr.rs) の `AssignIndex` が RHS を **ヒント無し**で lower し (`lower_expr(value)`)、 その後 slot 型へ **値全体を `coerce`** していた。 `coerce` は値自身の `T → T?` wrap しか見ず、 tuple の **内側要素** (`(Box, Box) → (Box?, Box)` の slot0) を wrap しないため、 生 Box が `Box?` slot に入り解放カスケードで不整列ポインタ参照 → crash。
- **修正**: `AssignIndex` の RHS を **slot の要素型 (Array の elem / Map の val) をヒントに `lower_composite_with_hint`** で lower (第 36 弾の bare field・第 25 弾の container store と同じ役割分担)。 tuple/array/map リテラルが宣言要素型で構築され、 slot0 が `lower_tuple_literal_with_hint` で wrap される。 非 composite RHS は従来どおり `lower_expr` にフォールバック。
- **同族確認**: array index 代入・Map index 代入の両方で `(Box?, Box)` slot0 wrap を Optional 越し match で実体確認しつつ deinit 厳密 (array 500/round・map churn)・delta=0。 array/Map の heap tuple リテラル・要素上書き (旧 tuple の Box 解放)・Optional<heap tuple> も probe して**元から健全**。
- fixture: `05_edge_cases/index_store_tuple_element_wrap.il` (array/Map の index 代入 tuple 要素 wrap を厳密 deinit 500 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 40 弾: tuple 再帰ギャップの同族 3 関数 — 第 39 弾の取りこぼし (2026-06-13)

第 39 弾は `subst_type` / `contains_type_var` (型パラメータ `T` の置換) に tuple arm を足したが、 **§8-6 同族探索が不完全**で、 同じ tuple 再帰ギャップを持つ rewrite 群を取りこぼしていた。 generic enum を probe する過程で顕在化:

- **症状**: `(Inner<i64>, i64)` (generic class) や `(Maybe<i64>, i64)` (generic enum) を **class field / fn param / fn return** に使うと **「mir lower: unsupported in M1: user-defined generic types」で停止**。 第 39 弾の `T` 置換と違い、 これは **concrete instantiation の mangle** (`Generic(Inner,[i64]) → Object("Inner_i64")`) の漏れ。 `(T, T)` の **local** 注釈や、 **tuple でない** `Inner<i64>` field は動く (前者は binding 型を rhs から再導出、 後者は tuple を経由しない) ため第 39 弾に隠れていた。
- **原因**: tuple を再帰しない関数が **3 つ**残っていた — [class.rs](../crates/ilang-mir/src/monomorphize/class.rs) の `rewrite_type` (user generic class を mangled Object 名へ)、 [enums.rs](../crates/ilang-mir/src/monomorphize/enums.rs) の `rewrite_enum_refs_in_type` (generic enum を mangle)、 [walk.rs](../crates/ilang-mir/src/monomorphize/walk.rs) の `walk_types_pre` (tuple 内にしか現れない instantiation の発見)。 いずれも `Type::Tuple` arm が無く `_ => clone()` / `_ => {}` に落ちていた。
- **修正**: 3 関数すべてに `Type::Tuple` arm を追加 (要素を再帰)。 `seed_enums_in_type` は `walk_types_pre` 経由なので自動的に直る。 monomorphize モジュールの型再帰関数 (`Type::Optional` を持つもの) を全数監査し、 残る漏れが無いことを確認。
- **同族確認**: generic class / generic enum が **heap Box を保持**して tuple field に入る形を deinit 厳密 (200/round) + churn delta=0、 nested `((Inner<i64>, i64), i64)`、 fn param/return、 generic enum tuple payload (`yep: (T)`) の再確認。 全て値正しく ARC 均衡。
- fixture: `05_edge_cases/generic_in_tuple_field.il` (generic class/enum を heap 込みで tuple field・param・return に置いて厳密 deinit 200 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 39 弾: monomorphize が tuple 型の中の型パラメータを置換しない (2026-06-13)

generic × heap × 第 36 弾の tuple wrap を probe する過程で発見:

- **症状**: generic fn / method のシグネチャ (param / return) や generic class の field に **tuple of T** (`(T, T)` / `(T?, T)` / `((T, T), T)`) を使うと、 呼び出し (= monomorphize) 時に **「mir lower: unknown type: T」で lowering 停止**。 `T?` / `T[]` / `Map<string, T>` は置換されるのに tuple だけ。 `let p: (T, T)` という **local 注釈は動く** (local はシグネチャに乗らない = monomorphize 対象外) ため見つかりにくかった。
- **原因**: [monomorphize/class.rs](../crates/ilang-mir/src/monomorphize/class.rs) の `subst_type` が Object/TypeVar・Generic・Array・Optional・Weak・Fn は再帰置換するが **`Type::Tuple` の arm が無く** `_ => t.clone()` に落ちて、 tuple の要素にある `Object("T")` がそのまま生き残る。 兄弟関数 `contains_type_var` も同じく Tuple を見落としており、 tuple を包む generic の mangle 判定も誤っていた。
- **修正**: 両関数に `Type::Tuple(elems)` arm を追加 (`subst_type` は各要素を再帰置換して再構築、 `contains_type_var` は要素のいずれかが型変数を含むか)。 Map は `Type::Generic` (base=Map) 経由で元から置換されていた。
- **同族確認**: generic fn の `(T, T)` param / return、 method 戻り値 `(T, T)`、 入れ子 `((T, T), T)`、 tuple 内 Optional/array `(T?, T[])`、 generic class の `(T?, T)` field を bare 代入 (monomorphize 後の T→T? 要素 wrap も含めて第 36 弾の経路と合流) — 全て値正しく ARC 厳密 (deinit 500/round + churn delta=0)。 generic 本体内で opaque T の `.n` には触れない (checker の既存制約、 バグではない)。
- fixture: `05_edge_cases/generic_tuple_type_param.il` (fn param/return・nested・generic class tuple field wrap を厳密 deinit 500 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 38 弾: async ARC 全方位 — 第 37 弾以外は健全 (2026-06-13)

第 37 弾の exit-drain 修正の周辺と、 触っていなかった async 経路を網羅 probe。 **新規バグなし**:

- **timer (setTimeout) at exit**: heap を捕獲した closure を持つ timer を未 tick で残しても、 exit 時の blocking drain (`pool::drain` の `TimerStep::Wait => sleep`) が期限を待って発火 → closure・heap を解放。 `time.sleep` + `time.tick` で観測すると log 反映 + deinit 1。 clean exit (第 37 弾の drain が timer 経路も覆う)。
- **Promise.all over heap** (3 個の `Promise.resolve(new Box(_))` を then で集約): 値正しく (1+2+3=6)・deinit 3・leak なし。
- **Promise を class field 保持** + then、 **multi-await chain** (`await fetch(10)` → `await fetch(20)`): 値・ARC とも健全。
- **never-settled promise が executor closure で heap Box を捕獲**して即捨て: churn 100 で deinit 100・delta=0 (promise/scope 死亡で capture も解放)。
- **await rejection が await 前の heap local を解放**: `risky(pr)` が `let guard = new Box(99)` の後 `await pr` で reject すると残り (`guard.n + v`) は実行されないが、 guard は解放される (第 11 弾の早期脱出 sweep が await-reject 経路でも効く)。 churn で guard 数と deinit が厳密一致・delta=0。
- **reject 経路の shutdown drain**: reject する promise を未 tick で exit に残し、 guard の deinit が top-level 配列に触っても **clean exit** (第 37 弾の drain は resolve だけでなく reject の継続も流す)。

fixture: `04_modules/await_reject_releases_pre_await_heap.il` (await-rejection の guard 解放を明示 tick で deinit 検証 + 未 tick の rejecting promise で reject 経路 shutdown drain を踏む)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 37 弾: exit 時の event-loop drain がグローバル解放の後に走り OOB panic (2026-06-13)

bare field 系統を離れ async を probe した初手で発見した**プロセス終了時の解放順序バグ**:

- **症状**: pending な Promise 継続が保持する heap オブジェクトの `deinit` が top-level グローバル (`deinits: i64[]`) に触ると、 プログラム終了時に **ランタイム OOB panic (`index out of bounds`、 Rust backtrace なし)**。 最小再現は「async fn が heap 返り async fn を `await` (Box を継続が跨いで保持)、 Box の `deinit` が top-level 配列にアクセス」で `.then` を 1 個登録するだけ (N=1 で再現)。 `time.tick()` で終了前にドレインすれば回避できる (= 順序の問題と確定)。
- **原因**: drain ([`$promise.drain`](../crates/ilang-runtime/src/promises.rs)) が **`__main` の return 後**に走っていた — JIT は [`run_main`](../crates/ilang-mir-codegen/src/compile/mod.rs) が `f()` の後に `__promise_drain()`、 AOT は C `main` ラッパが `entry` の後に呼ぶ。 ところが `__main` のエピローグは **return 前に top-level let を解放** する ([bodies.rs](../crates/ilang-mir/src/lower/decl/bodies.rs) の top_scope release)。 結果、 グローバル `deinits` 配列が先に free され、 その後の drain が継続を完了 → Box 解放 → `deinit` が解放済み配列を参照 → abort。
- **修正**: `__main` 本体の **top-level let 解放ループの直前**に `promise_drain` を emit ([bodies.rs](../crates/ilang-mir/src/lower/decl/bodies.rs))。 グローバル生存中に継続をドレインするので `deinit` が安全。 共有 MIR lowering 経由なので JIT / AOT 両方に効く。 外側の drain (`run_main` / AOT ラッパ) はキュー空で no-op として残置 (安全網)。 builtin 名は generic テーブルの `promise_drain` (`$promise.drain` ではない) を使用。
- **検証**: `time.tick()` で明示ドレインすると `deinit` 数が観測でき (3 promises → deinits=3、 100 churn → 100、 leak なし)、 未ドレインの pending promise を残しても **clean exit (panic 解消)**。 fixture: `04_modules/pending_promise_exit_drain_order.il` (明示 tick で deinit 数検証 + 未 tick の pending promise で shutdown drain を踏む)。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 36 弾: bare field 代入が composite リテラルの要素 wrap を欠いて SIGSEGV (2026-06-13)

第 34/35 弾で「bare field 代入は健全」と確認したが、 **composite リテラルの内側要素が wrap を要する**形は未踏だった。 enum payload / tuple を probe する過程で発見:

- **症状**: `pair = (a, b)` (bare、 field 型 `(Box?, Box)`) が **SIGSEGV (exit 139)**。 明示 `this.pair = (a, b)` は正常 (10)。 同型 array (`arr = [box]`、 field `Box?[]`)・map (`m = {"k": box}`、 field `Map<string, Box?>`) も同根。
- **原因**: bare field 代入は早期 lowering 済みの値を再利用する (第 33 弾) が、 その早期 lowering は target が field のとき **ヒント無し** (`target_ty` が None) で走るため、 tuple リテラル `(a, b)` が宣言要素型 `(Box?, Box)` でなく**推論型 `(Box, Box)` で構築**され、 slot 0 の `Box → Box?` 要素 wrap が起きない。 生 Box が `Box?` slot に入り、 解放カスケードが Optional として走査 → 不整列ポインタ参照でクラッシュ。 `store_value_to_field` の coerce は**値全体**の `T → T?` しか見ないため内側要素は救えない。 明示 AssignField は `lower_composite_with_hint(value, &fty)` を常に使うので無事だった。
- **修正**: bare field のとき (`target_ty` が None かつ `this_class` の field) に **field 型を composite リテラルのヒントとして供給** ([expr.rs](../crates/ilang-mir/src/lower/expr.rs) の Assign arm、 `bare_field_hint`)。 `lower_composite_with_hint` が tuple/array/map の要素を宣言型へ wrap して構築する。 外側 `T → T?` wrap は従来どおり `store_value_to_field` が担当 (役割分担: 内側 = リテラル構築ヒント、 外側 = store ヘルパー)。
- **同族確認**: tuple `(Box?, Box)`・array `Box?[]`・map `Map<string, Box?>` の bare 代入を、 各 Optional 越しの match で実体が壊れていないことを確認しつつ deinit 厳密 (500/round) + churn delta=0。 第 33〜35 弾の bare 代入 probe 群 (p5/q1/x1/s8) も全て回帰なし。 enum payload を field に持つ bare 代入・再代入 churn・match 持ち出し・Optional<enum>、 tuple の destructuring・入れ子 tuple return は別途 probe して**元から健全**。
- fixture: `05_edge_cases/bare_field_assign_composite_element_wrap.il` (tuple/array/map 要素 wrap を厳密 deinit 500 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 35 弾: bare field 書き × 反復ミューテーション・多段連鎖 — 新規バグなし (2026-06-13)

第 32 弾の bare field 書きと第 29/30 弾の連鎖レシーバ解放の継ぎ目を、 反復・escape・多段で攻めた。 **全て健全**:

- **init 内 bare 再代入の `is_init` 安全性**: implicit-this bare 代入 arm は `is_init` を常に `false` で渡す (= 旧スロットを LoadField して Release)。 一見「init のゼロスロットを誤 release」しそうだが、 **checker が「init での初回 field 代入は `this.field = ...` の形」を要求** するため (`slot = ..` だけの init は「field is not assigned」で拒否)、 bare 書きが届く時点でスロットは既に実値を持つ → 旧値 release は正しい。 `this.slot = a` の後 `slot = b` で再代入する形を deinit 厳密 (2/round) で確認。
- **fixed-array field の bare 代入** (`items = other.items`、 非 fresh エイリアス源): `store_value_to_field` の fixed-array arm が copyShallow で値コピー (新バッファ + 要素 retain)、 旧バッファ release。 deinit 厳密 (4/round)。
- **多段メソッド連鎖**: 4 段 `mk().bump().bump().bump()` の中間 receiver 3 個解放、 **field を返す連鎖** `h.getInner().val()` (tail-borrow で +1 された中間)、 self 連鎖、 fresh holder 連鎖。 全て deinit 厳密。
- **エスケープしたステートフルクロージャの heap field 反復付け替え**: `s.updater()` が返すクロージャを 4 回呼んで毎回 `slot = new Box(x)`。 各呼び出しで前の Box を release、 object + closure 死亡で最後を drop。 1 round 5 Box 全死亡を deinit 厳密で確認 (第 32 弾修正の反復ミューテーション下の堅牢性)。
- 計測で繰り返し出た `delta=56` は全て **§4-1 の罠** (計測開始後に確保した `acc: i64[]`)。 deinit が常に厳密一致でオブジェクト leak なし。

fixture: `05_edge_cases/closure_bare_field_reassign_churn.il` (ステートフルクロージャ反復 + init bare 再代入を厳密 deinit 700 + churn delta=0)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [確認済み記録] 第 34 弾: bare field 代入 × 宣言型が要る RHS — 新規バグなし + 第 33 弾の訂正 (2026-06-13)

第 33 弾で bare / 明示の field 代入を `store_value_to_field` に統合した直後、 その継ぎ目を「bare 名の field 代入に、 plain `T → T?` を超えて宣言型が要る RHS」で攻めた。 bare 経路は早期 lowering 済みの値 (field ヒント無し) を再利用するため、 ヒントが要る形が壊れないかを確認 — **全て健全**:

- **共変 map リテラル** `animals = {"d": new Dog()}` (field `Map<string, Animal>`)、 **共変配列リテラル** `members = [new Dog(), new Dog()]` (field `Animal[]`): 値正しく (Optional/コンテナ越しに `Dog.val()=10` でディスパッチ確認)、 deinit 厳密・churn delta=0。 ヒント無し lowering でも要素はオブジェクトなので runtime repr が一致し、 helper の store がそのまま通る。
- **fresh Optional RHS** `slot = makeOpt(7)` (Optional→Optional 同型、 二重 retain なし)、 **Optional widen** `pet = d` (`Dog? → Animal?`、 borrowed): deinit 厳密・delta=0。
- 計測で出た `delta=56` は **§4-1 の罠** (計測開始後に確保した `acc: i64[]` 自身) で、 `acc` を計測前に移すと delta=0。 deinit が常に厳密一致でオブジェクト leak が無いことは独立に確認済み。

**第 33 弾の誤記録を訂正**: 第 33 弾の同族確認に「weak field の bare 代入は checker が拒否・明示 `this.w = b` は通る非対称」と書いたが**誤り**。 実際に拒否されていたのは `this.w = none` (plain `Box.weak` を `none` で初期化) で、 これは **bare / 明示によらず同じ**挙動 (`w = b` strong→weak は両方通る)。 plain weak は nullable でなく、 nullable な weak は `Box.weak?` (Optional weak) を使う設計 — `Box.weak?` への `none` は通る。 docs/syntax_ja.md:988「空マップは `new Map<K,V>()` で構築 (`{}` は空ブロック扱い)」と同様、 これらは意図的な制限であってバグではない。

fixture: `05_edge_cases/bare_field_assign_composite_widen.il` (共変 map/配列・fresh Optional・widen を厳密 deinit 500 + churn delta=0)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 33 弾: implicit bare-name field 代入が Optional / subtype wrap を欠いて SIGSEGV (2026-06-13)

第 32 弾で bare field 代入の `this` 解決を直した直後、 同じ implicit `this.<field>` 代入 arm を「代入する値の型 × field 型」軸で攻めて **既存のクラッシュ**を検出。 明示 `this.f = v` (`AssignField` arm) は第 22 / 26 弾で `T → T?` / subtype の Optional 自動 wrap を入れたのに、 **bare 名の `f = v` 経路は退化した store を別に持っていて wrap を一切しなかった**。

- **症状**: `slot = box` (field 型 `Box?`、 `this.` なし) が **生オブジェクトを Optional slot に格納**し、 オブジェクト解放時のカスケードがそれを Optional として走査 → 不整列ポインタ参照で **SIGSEGV (exit 139)**。 明示 `this.slot = box` は正常 (AssignField が wrap する) という非対称で確定。 subtype (`Dog` を `Animal?` field へ) も同様。
- **原因**: bare field 代入は他の代入経路 (local / cell capture / repl slot) を全部抜けた後の implicit-this arm に **早期 lowering 時の生の `v` / `vty`** で到達し、 field 型への coerce/wrap を踏まずに `StoreField` していた。 AssignField が持つ wrap / weak / fixed-array value-copy / CReprEnum drop を全部欠いていた。
- **修正**: AssignField の store ロジック (wrap 述語 → fixed-array → retain/release → StoreField → CReprEnum drop) を共通ヘルパー **`store_value_to_field`** ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)) に抽出し、 **AssignField と implicit-this bare 代入の両方から呼ぶ**。 bare 経路は早期 lowering 済みの `v` / `vty` を再利用 (再 lower すると `new Box()` を二重確保するため) し、 ヘルパーの coerce が Optional wrap を行う。
- **同族確認**: `T → T?` 同型 wrap・subtype `Dog → Animal?` (Optional 越しの仮想ディスパッチ `Dog.val()=10` で実体が壊れていないことを確認)・**クロージャ内の bare Optional 代入** (第 32 弾の capture 経路と合流) を値・ARC 厳密一致 (deinit 300 + churn delta=0)。 fixed-array field の bare 代入 (fresh リテラル源) と primitive field は元から修正経路で健全。 strong→weak の bare 代入 (`w = b`、 field 型 `Box.weak`) は通る。 **(第 34 弾で訂正)** 当初ここに「weak の bare 代入は checker が拒否・明示は通る非対称」と書いたが誤り — 拒否されていたのは `this.w = none` (plain `Box.weak` を `none` で初期化) で、 bare/明示によらず同じ挙動。 plain weak は nullable でなく nullable weak は `Box.weak?` を使う設計 (第 34 弾で確認、 意図的制限)。
- fixture: `05_edge_cases/bare_field_assign_optional_wrap.il` (同型 wrap / subtype / closure 内を厳密 deinit 300 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 32 弾: メソッド内クロージャの `this` 解決が欠けていた 2 経路 — bare field 代入 (修正) と bare メソッド呼び出し (診断化) (2026-06-13)

第 31 弾で「メソッド内クロージャが field を **bare 名で読む**」経路に `this` の on-demand 捕獲を入れた。その同族として **`this` を必要とする残り 2 経路**が `lookup_var("this").unwrap()` を素のまま使っており、クロージャ内 (= `this` が local でなく capture 経由) で **`Option::unwrap()` on None のコンパイラ panic** を踏むことを probe で確認。両者とも「読み経路は capture フォールバックを持つのに、この経路は持たない」同根:

1. **bare field 代入 `slot = nb` が panic (修正)** — [expr.rs](../crates/ilang-mir/src/lower/expr.rs) の implicit `this.<field>` 代入 arm。 bare field **読み** (lower_var_expr) は `lookup_var → captures_in_scope` のフォールバックを持つのに、代入の store サイトは `lookup_var("this").unwrap()` 直書きだった。 `this` 自体は既に捕獲されている (代入 target が free var として収集され [collect.rs](../crates/ilang-mir/src/lower/collect.rs)、 `name_is_this_member` 経由で `this` が `frees` に入る) ので、 **読み経路と同じフォールバックを store サイトにミラーするだけ**で動く。 ARC は従来どおり (旧値 release・新値 own)。 fixture: `05_edge_cases/closure_bare_field_assign.il` (heap field 付け替え + primitive field を厳密 deinit 200 + churn delta=0)。

2. **bare メソッド呼び出し `compute()` が panic → クリーンな診断 (ユーザー決定)** — [call_fn.rs](../crates/ilang-mir/src/lower/call_fn.rs) の implicit `this.<method>(args)` arm。 第 31 弾の記録にあった「メソッドを `name_is_this_member` に含めると新規 panic を生む」の正体がこの素の `unwrap()` だった (含めると `this` 捕獲が起きてこの arm に到達し panic)。 **ユーザー判断 = メソッドは capture 対象外の既存決定を維持し、 panic を明示エラーに置換** (「bare method call \`compute(...)\` inside a closure is not supported; write \`this.compute(...)\`」)。 明示形 `this.compute()` はクロージャ内でも従来どおり動く。 fixture: `05_edge_cases/closure_bare_method_call_error.il` (expect-error)。

同族確認: 同型の素 `unwrap()` は grep で上記 2 箇所のみ。 bare field の field-of-field 書き (`box.n = ..`) は AssignField 経由で obj が読み経路を通るため元から健全、 primitive field 書きも修正経路で正しい。 syntax.md / syntax_ja.md のクロージャ節に「メソッド内クロージャの `this` メンバ参照」を追記。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] gui ライブラリが platform イベントループから `time.tick()` を pump するようにした (2026-06-11、 ユーザー判断 = 案 (a))

JS 型移行の積み残しだった「`gui.run()` がネイティブループでブロックして drain ポイントを通らず、 GUI アプリの `time.setTimeout` / `setInterval` / Promise 継続がイベントループ実行中に発火しない」問題。 3 プラットフォームとも `runEventLoop` に ~15ms 周期の platform タイマーを仕込んで `time.tick()` を呼ぶ形で解消:

- **cocoa** ([libs/gui/cocoa/window.il](../libs/gui/cocoa/window.il)): repeating `NSTimer` (0.015s、 tolerance 0.005s) を **common modes** (`kCFRunLoopCommonModes`) で main run loop に登録。 default mode だとウィンドウドラッグ / メニュー追跡中に止まるため
- **win32** ([libs/gui/win32/window.il](../libs/gui/win32/window.il)): `SetTimer` を **TIMERPROC 付き** (`SetTimerWithProc` — `@symbol("SetTimer")` で別シグネチャを [bindings/windows/user32.il](../bindings/windows/user32.il) に追加) で登録。 NULL-hwnd の queue タイマーはモーダルループ (ドラッグ / メニュー) 中に捨てられるが、 TIMERPROC は `DispatchMessage` が直接呼ぶので届く。 加えてメッセージ dispatch ごとに `time.tick()` (0ms timeout / 新規継続の即時サービス)。 ループ終了後 `KillTimer`
- **linux** ([libs/gui/linux/window.il](../libs/gui/linux/window.il)): `g_timeout_add(15, linux_pump_tick, ...)`。 `g_timeout_add` は GSourceFunc callback 型のため GIR generator が出せず、 [bindings/gtk4/manual.il](../bindings/gtk4/manual.il) に手書き追加 (signal-connect 変種と同じ理由)

**検証**: macOS は実機確認済み — headless の Foundation run loop テスト (`NSTimer` common modes pump + `NSRunLoop.runUntilDate` 中に `setTimeout` 発火、 AppKit/ウィンドウなし) と `window_state_demo` の AOT ビルド。 **win32 / linux はこの macOS 環境では型検査自体が走らない** (gui_impl の per-OS deps で cocoa しか load されない + gtk4 / pkg-config なし) ため未検証 — 既存コードの型付け慣例 (callback は top-level `pub fn` を名前渡し、 i32 戻りは `0 as i32` 形) に合わせて書いたが、 Windows / Linux 機で `gtk4_bindings` テストと GUI ビルドを通すこと。

タイマー分解能は pump 周期の ~15ms (gui.run の doc コメントに明記)。 sdl_breakout のような自前フレームループのアプリは従来どおり自分で `time.tick()` を呼ぶ。

### [解決済み記録] fixture 増殖ラウンド第 10 弾: Promise.all/race の入力配列・結果配列の所有権、fresh promise 引数 (2026-06-11)

probe 対象を Promise.all/race の ARC (途中 reject の部分値・race 敗者の値・settle されないまま捨てる形)、 enum/tuple payload の promise、 promise を持つ容器 / class field + deinit、 await × deferred の churn、 REPL チャンク跨ぎへ広げた。 3 系統のバグ (すべて移行前から — ただし 2 は第 9 弾までの修正で顕在化):

1. **`Promise.all` / `race` が入力配列を消費しない** (既存)。 lowering ([builtin_static.rs](../crates/ilang-mir/src/lower/calls/builtin_static.rs) `lower_promise_combinator`) は executor と同じ譲渡規約 (非 fresh に Retain) なのに runtime が release せず、 呼び出しごとに入力配列が leak。 fresh literal 入力では upstream promise (と settled 値) も配列に pin されたまま永久に残った。 修正: `__promise_all` / `__promise_race` が登録完了後に `release_input_array` (KIND_ARRAY cascade)。 pending upstream は「escape した resolve callback の capture」経由で生存するか、 waiter ごと死ぬ (発火し得ないので観測上同一)。
2. **`Promise.all` の結果配列が要素の所有権を持っていなかった** (既存の潜在バグ — 1 の修正で UAF として顕在化)。 `build_i64_array` は raw slot を retain せずコピーするのに、 resolve stub が slot retain を構築後に release しており、 要素の所有者が不在。 旧実装では leak した upstream の Resolved state が偶然値を支えて読めていたが、 入力配列の消費開始で upstream が正しく死ぬと **heap 要素の `vs[i]` が解放済みメモリ** (`vs[0].n` が非決定な garbage)。 修正: slot retain をそのまま結果配列の要素所有分として移譲 (release ループを削除)。 reject 経路の「部分 slot の解放」はそのまま正しい。
3. **fresh promise を引数で渡すと leak — `needs_post_release` の 5 サイト全部に `Promise` が無かった** (既存)。 named-fn / closure / 暗黙 this / object method / `new` init のリストがそれぞれ手書きコピーで、 全部 Promise を欠いていた (init サイトには「promise field は retain しない」という**古い誤コメント**まで付いていた — 実際は `is_arc_slot` が Promise を含み field assign は retain する)。 `new Holder(Promise.resolve(new Box(k)))` で promise + settled 値が 1 個/call leak。 修正: 共通述語 `BodyCx::fresh_arg_needs_post_release` に一本化して 5 サイトを置き換え (Promise 込み)。

**probe 中に判明した独立の既存制限**: 対話 REPL は `use` 文を受け付けない (`<repl> mir: unexpected Item::Use post-loader`)。 std モジュール (time 等) が REPL から使えない。 async とは独立の機能ギャップとして未対応のまま (対応するか要ユーザー判断)。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 回帰 fixture 3 件: `04_modules/promise_all_race_input_arc.il` (入力 churn 5 変種)、 `04_modules/promise_all_object_values.il` (heap 要素の値正しさ + churn — UAF の pin)、 `04_modules/promise_fresh_arg_release.il` (init / fn 引数の fresh promise churn + deinit 数)。

**probe で問題なしを確認した周辺** (再調査不要): enum payload (heap 側) の promise churn、 Map / 配列に promise を格納して捨てる churn、 promise class field + deinit (修正後 delta=0・deinit 200/200)、 await × deferred 400 周 (定数 56 bytes の一回きり初期化のみ、 線形成分なし)、 REPL のチャンク跨ぎ promise drain (`then` が後続チャンク前に発火)。

### [解決済み記録] 第 19 弾: 固定長配列 `T[N]` × heap 要素を正式サポート — 選択肢 (a) を実装 (2026-06-12)

第 18 弾の判断待ちに対しユーザーが **(a) フルサポート** を選択。実装した所有権モデル (ポインタ転送 + 値セマンティクス):

- **レイアウトの確定事実**: 固定長配列はヘッダ無しの `len*8` バッファ。非 CRepr class の field は **8 バイトのポインタスロット** (インライン展開は CRepr struct のみ — 当初インライン前提で書きかけて codegen 読解で訂正)。注釈なしリテラルは checker 上 `T[3]` に推論されても **lowering では動的配列に decay** する (`len_hint` は注釈起点でのみ立つ) — fixed の経路に入るのは注釈・field 型・param 型経由のみ。
- **所有権**: fresh リテラルを束縛した binding がバッファ + 要素 share を所有 ([stmt.rs](../crates/ilang-mir/src/lower/stmt.rs) が `fixed_owned_locals` に登録、 `crepr_owned_locals` と同型)。`let v = p.items` や param 渡しは**借用** — scope-exit / 早期脱出 sweep は owned のみ release ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs) / [control.rs](../crates/ilang-mir/src/lower/control.rs) のゲート)。
- **Release(fixed-of-arc)** は `$array.releaseFixed(ptr, len, elem_kind)` (要素 release + バッファ free) に lower ([arc.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/arc.rs))。primitive / CRepr 要素 (`u16[128]`, `Vertex[3]`) は従来どおり no-op (stride も 8 でないため)。
- **field 代入** ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)): fresh リテラルは**ポインタ転送** (rc 操作ゼロ)、 非 fresh ソースは `$array.copyFixed` で**値コピー** (新バッファ + 要素 retain — ポインタを直接書くと 2 owner が 1 バッファを共有し二重 free)。旧 region は Release で要素 + バッファ解放 (init 書きはスキップ)。
- **object drop cascade**: field kind 表に**合成タグ** `KIND_FIXED_BASE(=1000) + len*16 + elem_kind` を導入 ([kind.rs](../crates/ilang-runtime/src/kind.rs) / [cascade.rs](../crates/ilang-runtime/src/cascade.rs) で decode、 JIT [print_kind.rs](../crates/ilang-mir-codegen/src/compile/print_kind.rs) `kind_tag_of` / AOT [helpers.rs](../crates/ilang-mir-codegen/src/aot/helpers.rs) `field_kind_tag` — AOT 側は classes 引数を追加し CRepr/handle 要素を 0 に揃えた)。第 18 弾の「per-slot 展開が要る」という見立てはポインタ格納の判明で不要になった。
- **置き場所の制限** (checker): 固定長 heap 配列は **class field / ローカル束縛 / fn param のみ**。戻り値型・他コンテナの構成要素 (`validate_type` の各 composite arm + tuple/some/配列/Map リテラルの式レベル検査 — **配列リテラル要素は decay するので除外**、これを忘れて既存 fixture 2 本を誤って落とした)・closure capture ([fn_expr.rs](../crates/ilang-mir/src/lower/fn_expr.rs) の lowering ガード)・束縛全体の再代入は型エラー。
- **JIT シンボル**: `$array.releaseFixed` / `$array.copyFixed` を [jit_symbols.rs](../crates/ilang-mir-codegen/src/compile/jit_symbols.rs) に明示登録 (releaseFixed は Rust 側参照で偶然リンクされていたが copyFixed は dead-strip され「can't resolve symbol」になった)。

**probe 結果 (全て正確な deinit 数 / delta=0)**: ローカル scope exit 400/400、 要素上書き 3/call、 field drop cascade 400/400 (deinit 連鎖 1002 = 1000+2 まで正確)、 field 上書き 2、 値コピーの独立性 (`p` 不変)、 借用 (alias read / param) は deinit 0、 早期 return 100/100、 `.length` / for-in / `console.log` 表示も正常。probe の「leak に見えた定数 56 bytes」は計測後に確保した `acc: i64[]` 自身だった (計測順の罠、 第 9 弾の教訓と同種)。

**後続で踏んだ二重解放 (修正済み)**: トップレベルの `let m = teamA.members` (別名) が**プロセス終了時に SIGABRT**。`__main` のエピローグは in-fn の scope sweep (`release_binding_for_scope_exit`) と**別の独自 release ループ** ([decl/bodies.rs](../crates/ilang-mir/src/lower/decl/bodies.rs) の top_scope 走査) を持っており、そちらが未ゲートだった — 別名 release がバッファと要素を先に解放し、直後の object cascade が解放済みバッファを再走査して abort。owner 判定 (`fixed_owned_locals`) のゲートをエピローグの Local/Ssa arm にも追加、slot 一掃にも同型のゲート (`fixed_owned_slots`) を追加。説明用のドキュメント例を実行して発覚 (probe は fn 内ローカルしか張っていなかった)。fixture: `05_edge_cases/fixed_array_top_level_alias.il`。**教訓: 解放経路は 1 本ではない — `release_binding_for_scope_exit` / `__main` エピローグ / slot 一掃 / break sweep の 4 箇所すべてに同じ所有ゲートが要る。**

**内部表現を rc 付きへ切替 + 全セル対応 (ユーザー承認)**: tuple のセル kind が 4bit 詰めで合成タグが入らない問題を機に、**fixed-of-arc の内部表現を動的配列と同一 (ヘッダ + rc) に変更**。見える挙動は案 A のまま (セル格納時の値コピーは `$array.copyShallow` をコンパイラが挿入)。primitive / CRepr 要素 (`u16[128]`, `Vertex[3]`) は従来のヘッダ無し inline を維持 — 分岐は全箇所 `kind_tag_of(elem) == 0`。これにより: (1) **tuple / 動的配列要素 / Map 値 / closure capture / Promise 値が全部対応** (ランタイムのセル機構は無変更 — 普通の KIND_ARRAY として扱われる)、(2) 合成タグ (`KIND_FIXED_BASE`)・`$array.releaseFixed` / `copyFixed`・所有者追跡 (`fixed_owned_locals` / `fixed_owned_slots`)・4 箇所の解放ゲートを**全削除** (固定長は通常 heap let の retain/release 均衡に乗る — エイリアスが所有者より長生きしても安全)、(3) print / fmt / builtin ラッパ (`fixed_to_dyn`) は arc 要素ならヘッダ経由に分岐。コピー挿入サイト: field 代入・some×3・enum ctor×2・tuple literal×2・配列 literal×3 系統・AssignIndex・push / unshift・map literal / set・Promise.resolve・capture 収集。**残る型エラー**: 後続セッションで全て解除済み — 戻り値と再代入は読み出し側の共有セマンティクス (generic の return 機構と Assign の heap 経路にそのまま乗る)、`Array.fill` は `$array.fillCopy` (スロットごとに shallow copy)。解除に伴い `copy_fixed_for_cell` に freshness を導入 (fresh な call 結果はコピーせず transfer — コピーすると元の +1 が宙に浮く)。**ついでに既存 leak を 1 件検出・修正**: `let o: Box? = makeBox()` の T→T? auto-coerce が fresh 元値の +1 を誰も解放しておらず 1 個/call leak (fixed と無関係の一般 heap で再現・修正、stmt.rs の coerce 後に Release)。fixture: `fixed_array_heap_elem_return_reassign_fill.il` (placement_error を改名・全面書き換え)。**既知の意味論エッジ (記録のみ)**: `.then` / `array.map` の**コールバックが固定長配列をそのまま返す**形は結果セルが共有になる (rc で健全、値セマンティクスのみ破れ — callback 結果の格納が runtime 内のため。対応するなら runtime の結果格納にコピーを足す)。fixture: `fixed_array_heap_elem_cells.il` (全セル + churn 100 周 delta=0)、placement_error は「戻り値」ケースに書き換え。検証: 既存 fixed fixture 全行が**表現切替前と同一出力** (意味論保存)、nextest 539/539、AOT PASS、nested_generic 100 並列 0 fail。

**generic 対応 (案 A、ユーザー決定)**: `fn hold<T>(v: T): T? { some(v) }` に `Box[2]` を渡せるようにした。規約は「**コンテナのセルは格納時に値コピー**」(field 代入と同じ): `copy_fixed_for_cell` ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs)) を Optional 構築 3 箇所 (Some / hint付き / T→T? coerce) + enum ctor payload 2 分岐に挿入。取り出し (match / unwrap) はコピーを指す。owner 判定は「リテラル右辺の let」に限定 (call 結果の fixed は常にエイリアス — generic 恒等関数が呼び出し元のバッファを返すため、 fresh 判定で owner 扱いすると二重 free)。型引数の一律拒否 (`reject_fixed_heap_type_arg`) と Optional/enum payload の注釈拒否を撤去。未対応のセル (tuple slot / 動的配列要素 / Map / Promise 値 / 再代入) は **lowering ガード** `forbid_fixed_in_cell` で明示診断 — checker は generic body を T 不透明で検査するため、mono 後に届く形はここで止める。probe: hold の独立性 (a を書いても o に見えない)・deinit 数 (3/2/2 で正確)・200 周 churn delta=0 deinits=400。fixture: `fixed_array_heap_elem_generic_arg.il` (旧 expect-error を正値テストに書き換え)。tuple の対応には cell kind の 4bit 詰めの拡張が要る (保留)。

**既知の残り**: REPL slot は固定長配列を正しく扱えない (`xs.length` がバッファ先頭を length と誤読 — **HEAD でも同じ挙動の既存ギャップ**、stash して確認済み)。

**generic 型引数のすり抜けは後続コミットで封鎖済み**: `fn hold<T>(v: T): T? { some(v) }` に `Box[2]` を渡すと T が fixed に推論され、 mono 後の Optional cell が借用バッファを own して **SIGABRT (二重 free) を実機再現**。 generic body は T を不透明として検査済みのため per-instantiation の再検査はできず、 「型パラメータは固定長 heap 配列に解決できない」を一律ルールに。 検査点は推論の合流点 4 箇所: generic fn 呼び出し ([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs))・generic メソッド ([method.rs](../crates/ilang-types/src/checker/method.rs))・enum ctor ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs))・`new C<Box[2]>` の明示型引数 (check_new)。 ヘルパーは `reject_fixed_heap_type_arg`。 fixture: `05_edge_cases/fixed_array_heap_elem_generic_arg_error.il`。 直接 `Box[2]` と書く param は従来どおり可 (借用)。

**検証**: workspace nextest 539/539 (placement エラー fixture +1 込み)、 AOT arm 全 fixture PASS、 nested_generic 儀式 JIT 400 + AOT 400 で 0 fail。fixture: `05_edge_cases/fixed_array_heap_elem_store.il` を包括版に書き換え (10 系統を 1 本で pin)、 `05_edge_cases/fixed_array_heap_elem_placement_error.il` 新設。`docs/syntax.md` / `syntax_ja.md` の配列節に heap 要素の所有権と置き場所制限を追記。

### [解決済み記録] 第 18 弾: 固定長配列 `T[N]` × heap 要素 — ARC 未モデルの自認ギャップが実在 (2026-06-12、 第 19 弾で解決)

codegen の既存コメント ([arc.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/arc.rs): 「fixed-length arrays skip rc bookkeeping entirely」、 [helpers.rs](../crates/ilang-mir-codegen/src/aot/helpers.rs): 「Heap-element fixed arrays would still need per-element release, which isn't modeled here yet」) が**実害として確認**できた回。 probe で確定した表面マップ:

**正しく動く** (fixture `05_edge_cases/fixed_array_heap_elem_store.il` で pin 済み): `arr[i] = x` の旧要素 release / 新要素 retain (deinit が上書きごとにちょうど 1 回)、 要素を束縛へ読み出して配列より長生きさせる形。

**漏れる** (すべて deinit 0 / 64 B/call 級): ローカル `Box[2]` の scope exit、 早期 return の sweep、 `class Pack { items: Box[2] }` の drop cascade (field kind が KIND_NONE 登録)、 field 上書き (`p.items = [..]` で旧要素が漏れる)。 string 要素 (`string[2]`) も同様に 2 個/call。

**修正が単純でない理由 — inline 配列の所有権モデルが未定義**: 固定長配列は rc ヘッダの無い inline 値データで、 (a) `let copy = arr` は**ポインタのエイリアス** (要素 release を sweep に足すだけだと二重解放)、 (b) `return arr` の所有権 (borrowed-retain は no-op に落ちる)、 (c) コンテナ内 (dyn 配列の要素 / enum payload / tuple / Map 値 / Optional) は kind テーブルがスカラーで「N 要素 × kind-K の inline 配列」を表現できない。 修正するなら: scope-exit/early-exit sweep の per-element release + 非 fresh let の借用 (PatternBinding) 化 + AssignField の旧要素 release / 非 fresh 代入の per-element retain + class field 登録の per-slot kind 展開 (JIT/AOT 両方) + return の per-element retain、 を一貫して入れる必要がある。

**選択肢 (ユーザー判断待ち)**: (a) 上記フルセットで heap 要素を正式サポート (所有権モデルの設計込み)、 (b) **型検査で「固定長配列の要素は primitive のみ」と制限** — 1 診断で健全性を回復する小さな修正 (heap が要るなら動的配列 `T[]` を使う、 という案内)、 (c) 現状を文書化して放置 (leak は残る)。 (b) が最小で健全、 (a) は機能価値あり。

### [確認済み記録] 第 17 弾: weak × sweep・property 本体・fs/path/Unicode・interface 配列 — 全 probe 問題なし (2026-06-12)

probe 対象: weak field を含む fn の早期 return (strong count 不変・deinit 0)、 property getter/setter **本体**の早期 return + heap ローカル (accessor も fn body — sweep 適用、 strings=0)、 string-repr enum / @flags enum、 fs のエラー経路 churn (FsError payload の解放 ✓ delta 定数・strings=0)、 fs の Unicode 内容 + Unicode ファイル名ラウンドトリップ、 path の端入力 (空文字 basename / ルート dirname / `..`・`.` 正規化 / join の空要素 / dotfile extname / relative)、 string の多バイト操作 (絵文字の length / charAt / slice / indexOf / split)、 **interface 型 fresh 配列を for-in + return で貫通** (vtable 分発 + 要素 deinit ちょうど 2/call)、 generic class が interface 値を保持する churn、 tuple を Map 鍵にした時の診断 (正規メッセージ ✓)。 **新規バグなし** — 検出は全て probe 側のミス (interface は nominal で `class X: Iface` 宣言が必須、 という言語仕様どおりの拒否を誤検出しかけた)。

fixture: `05_edge_cases/early_exit_weak_property_iface.il` (weak / property / interface×for-in×return / repr・flags enum を 1 本で pin)。 コンパイラ変更なし — workspace nextest 539/539 + AOT arm PASS、 儀式は省略。

### [解決済み記録] 第 16 弾: REPL の再定義セマンティクス — 型違い re-let の健全性穴を封鎖 (2026-06-12)

REPL の再定義を probe した回。 1 件修正、 2 件は設計判断待ち:

1. **型違い re-let が無言の型穴だった (修正済み)**。 `let x = 41` → `let x = "str"` → `console.log(x)` が **string ポインタの生値を印字** — slot_table が旧型 (i64) のまま新値の bit が slot に書かれ、 次の読みが再解釈していた。 同型 re-let は正常な上書き (REPL の通常ワークフロー) なので維持し、 **型が変わる re-let は明示エラー** (「use a new name」) に。 repl.rs +3 件 (同型上書き / 型違い拒否+旧値生存 / @derive 動作確認)。
2. **fn / class の再定義は設計判断待ち**。 fn 再定義は「duplicate overload」拒否で旧定義が残る (再定義不可)。 class 再定義は cranelift の「incompatible signature」という生エラー (旧 class が残る)。 enum 再定義は**偶然** last-wins で動く。 一般的な REPL は last-wins 置換だが、 class 置換は「旧 layout で作られた既存インスタンス × 新 layout のメソッド」という未定義動作を作るため、 置換するなら旧型 slot の扱い (無効化 or 警告) を決める必要がある — ユーザーと相談。
3. **chunk 内の runtime panic は REPL セッションごと落ちる** (限界として記録)。 panic 後の slot 状態は信頼できないため、 復帰させるのも危うい。 file モードと同じ「panic = プロセス終了」。

@derive(Eq, Hash) は REPL で機能することを確認 (第 15 弾の normalize 乗せ替えの恩恵)。 promise の `.then` は後続 chunk の drain で発火し、 エラー chunk を挟んでも復帰する。

**検証**: repl 14/14、 workspace nextest 539/539。 CLI のみの変更 (lowering 不変) のため AOT arm / nested_generic 儀式は省略。

### [解決済み記録] 第 15 弾: REPL を loader 相当の normalize 経路に乗せ替え — enum / async / const / generic slot / use を修復 (2026-06-12)

「別領域 (REPL・capability・未実装機能の境界)」を狙った回。 まず境界 probe の結果: `@requires(net, file.read)` は意図どおり「パースされ未 enforce」 (ilang-types/lib.rs に明記あり)。 **未知の属性 (`@bogus`) は無言で素通り** — typo した属性が静かに無効になる footgun として記録 (**後続セッション 2026-06-12 で解決**: `parse_attributes` に正規セット 17 種の whitelist を入れ、未知名は既知一覧つきパースエラー。コンパイラ合成の内部属性 `sel` / `byValue` / `variadic` はパースを通らないため対象外。fixture: `05_edge_cases/unknown_attribute_error.il`)。 `?` 演算子は **HANDOFF の「未実装」リストが古く、 Result<T, E> 用に実装済み** — ok/err 伝播・heap payload・err 経路で heap let を跨ぐ形 (第 11 弾 sweep が遡って直していた)・deinit 数まで全て正確なことを probe で確認。 `x?` を Optional に使うと「Optional has no variant "ok"」という分かりにくい診断になる点は記録 (**後続セッション 2026-06-12 で対応**: ユーザー決定により `?` を Optional に対応 — none で `return none`、some で展開。パーサの `?` 脱糖は型を知らず Result 形 (`ok/err` arm) を吐くため、checker ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) と lowering ([match_.rs](../crates/ilang-mir/src/lower/match_.rs)) が合成名 `__try_*` の scrutinee で形を識別して some/none に差し替える。囲み fn は Optional 返しの同期 fn 限定 — async 内は normalize が先に err arm を settle に書き換えるため明示診断で拒否。**この作業で Result `?` の既存 use-after-free を発見・修正**: 脱糖ブロックの `__try` 一時束縛が scope exit で cell を解放するのが、外側 `let v = e?` の retain より先 — heap payload を `?` の後で読むと解放済みメモリだった (第 15 弾の probe は payload を脱糖ブロックの外に持ち出していなかった)。裸 Var の arm 本体はブロック末尾の retain 対と組まないため、arm 内で明示 Retain を発行 (`force_tail_retain`)。併せて is_fresh_object_expr の Match 判定で diverge する arm (return/break/continue) を除外 — `?` の伝播 arm が「全 arm fresh」判定を壊して二重 retain leak になっていた。fixture: `05_edge_cases/question_op_optional.il`)。 throw / try / labeled break は正規の parse error。 generic fn は推論呼び出し (`id(5)`) が動作、 turbofish (`id<i64>(5)`) は比較式に誤パースされ「undefined variable "i64"」になる点を記録。

**REPL の本丸 — 根本原因は「loader も normalize も通らない」パイプライン**。 検出した実バグ: (1) enum が chunk を跨いで使えない (`E.a` の Field→EnumCtor 書き換えが normalize 内のため)、 (2) async fn が旧診断で全滅、 (3) `const` が Item::Const のまま MIR に到達、 (4) generic 型 slot (`Result<i64,string>` / `Box<i64>`) が**無言で**永続化失敗 → 次 chunk で素の "unbound variable"、 (5) `use` が "unexpected Item::Use post-loader"。 **修復** ([main.rs::run_chunk](../crates/ilang-cli/src/main.rs) 再構成):

- `loader::normalize_repl_chunk` 新設 — load_program_full 末尾と同じ連鎖 (enum-ref renormalize → @derive 展開 → const inline → async desugar) を in-memory プログラムに適用 (auto_lift_objc と dup_pub は対象外と明記)
- **chunk ごとに fresh TypeChecker** で「accumulated 生 items + chunk」の正規化済み全体を検査 (永続 checker での再検査は duplicate-overload ガードに当たる)。 過去 chunk の let は slot_table から `TypeChecker::define_global` (新設) で seed
- **slot 型を monomorphize の追加要求として注入** — `monomorphize_with_requests` / `monomorphize_enums_with_requests` (新設、 既存入口は空要求で委譲)。 読みしかない chunk には instantiation site が無く、 特殊化 class/enum が合成されないため。 lower の slot 解決にも Generic→mangled 名のフォールバックを追加 ([lower/mod.rs](../crates/ilang-mir/src/lower/mod.rs) 3b、 `monomorphize::mangle_generic_name` 公開)
- slot 化できない let は **stderr に「persist しない」注記** (旧: 無言)
- `use` は「REPL 未対応、 `ilang run` を使え」の明示診断 (file loader のモジュール解決が必要なため未実装のまま — 対応するなら loader の overlay 機構で chunk を仮想 entry 化する方向)

**成果**: REPL で enum / async fn / const / `Result<i64,string>`・`Box<i64>` 型 let の chunk 跨ぎがすべて動作。 repl.rs に回帰テスト 6 件追加 (計 11)。 REPL は行単位入力のため複数行構文が書けない点は別件の使い勝手課題として残る。

**検証**: workspace nextest 536/536、 AOT arm 全 fixture PASS、 nested_generic.il 0/800 (lower の slot 解決に触れたため実施)。

### [確認済み記録] 第 14 弾: 早期脱出 × 値の持ち出し — 全 probe 問題なし (2026-06-12)

第 13 弾の教訓 (leak probe は過剰解放を見落とす) を踏まえ、 **deinit カウントを過剰解放検出器**として「sweep が掃除する出口を値が通り抜ける」形を網羅した回。 **新規バグなし** — 以下すべて値・leak・deinit 数まで正確:

- `return b` (fresh scrutinee の payload を返す — sweep の scrutinee 解放より先に borrowed-return retain が立つ)
- `break b` (payload を loop の外へ — lower_break の borrowed retain が sweep に先行)
- mutating-capture closure の早期 return (scope の cell share だけが落ち、 escape した closure は自分の share で生存)
- 入れ子 fresh-iterable for-in を 1 つの return で貫通 (両 iterable 解放、 全要素 deinit ちょうど 1 回)
- async 本体内の match arm / while / for-in からの return (await 前後どちらも settle 正常)
- async-match churn / cell churn / payload churn すべて delta=0

**適切な診断付きの既知制限を 1 件確認**: match の**ある arm に await・別の arm に return** がある形は state-machine lowering 未対応で、 「Refactor to a supported shape」の正規診断が出る (誤動作ではない。 await を match の前に出せば全 arm で return 可)。

fixture: `05_edge_cases/early_exit_payload_escape.il` (上記全部を 1 本で pin)。 lowering 変更なしのため nested_generic 儀式は省略、 workspace nextest 530/530 + AOT arm PASS。

### [解決済み記録] 第 13 弾: for-in × 早期 return の両方向 — fresh iterable の leak と要素借用の過剰解放 (第 11 弾 sweep の退行) (2026-06-12)

probe 対象を for-in over Map/Set (`entries()`/`values()`) の途中 break/return、 if-let + 早期 return、 tuple 分解 + 早期 return へ広げた。 2 件 (1 は既存、 1 は**第 11 弾 sweep が main に入れた退行**):

1. **fresh iterable が `return` 経路で leak** (既存)。 for-in の fresh iterable (`for e in m.entries()`) の Release は loop の exit block にのみあり、 `break` は exit を通るが `return` は通らない — entries 配列一式 (~144 B/call) が leak。 `live_fresh_scrutinees` の機構に iterable も登録 (loop frame の**外側の depth** で push するので break/continue の sweep からは除外され、 exit block の既存 Release と二重にならない。 ilang にラベル付き break が無いため break が for-in 境界を跨ぐことはない)。
2. **for-in の要素束縛を return sweep が過剰解放** (第 11 弾の退行、 **2026-06-11 の `4abb94d8` から main に入っていた**)。 要素束縛は ArrayLoad の借用 (retain なし) なのに素の `Ssa` で env 登録されており、 第 11 弾の return sweep が「owned binding」として release — 長寿命配列の要素が配列より先に死ぬ UAF (`for e in m.entries() { ... return }` の SIGABRT で発覚。 非 fresh 配列でも同型で、 **昨日の probe は leak だけ見て過剰解放を見落としていた**)。 要素束縛を `PatternBinding` (match payload と同じ「容器が解放責任を持つ借用」、 sweep 除外) に変更。 `env.bind` の他の呼び出し箇所は監査済み — LetTuple / LetStruct の束縛は retain 済みの owned、 receiver temp も owned、 range の loop var は int で、 誤分類はこの 1 箇所だけ。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 fixture: `05_edge_cases/forin_early_return_arc.il` (fresh iterable の回収 + 非 fresh 配列の要素が 200 回の早期 return 走査後も生存・deinit がちょうど要素数、 の両方向を pin)。

**probe で問題なしを確認した周辺** (再調査不要): Set `values()` + break、 if-let (fresh scrutinee) + 早期 return、 tuple 分解 (`let (a, b) = makePair()`) + 早期 return。

### [解決済み記録] 第 12 弾: continue の sweep・fresh scrutinee の早期脱出・string match の scrutinee・async の早期 return (2026-06-11)

第 11 弾の `return` sweep の同族を狙った回。 4 系統 (3 は既存バグ、 1 は未対応構文の実装):

1. **`continue` も sweep 無し** (既存)。 loop body の heap `let` を跨ぐ `continue` が 1 個/skip leak。 `lower_continue` に break と同じ「loop frame 以降の scope sweep」を追加 ([control.rs](../crates/ilang-mir/src/lower/control.rs)、 per-binding 規則は return と共通の `release_binding_for_scope_exit`)。
2. **fresh scrutinee の match で arm が diverge すると scrutinee が leak** (既存)。 arm 末尾の `Release(scrutinee)` は `!diverges` ガード付きで、 return/break で抜けると誰も解放しない (enum 64 B/call 等)。 `BodyCx::live_fresh_scrutinees` (ValueId + 登録時 env depth のスタック) を追加し、 enum / optional / if-let / string match の arm 本体 lowering 中に push、 3 つの早期脱出 sweep が「跨いで出る分」 (return = 全部、 loop jump = loop frame depth 以深) を release。
3. **string match は fresh scrutinee を一切 release していなかった** (既存・diverge 無関係)。 `match "k" + s { ... }` が素通りでも 1 string/評価 leak。 `lower_match_str` に enum と同じ freshness 規則を実装。
4. **async fn 本体の早期 `return` を実装** (旧: poll fn の `()` 戻りと衝突する誤解を招く型エラーで不支持)。 desugar が poll fn へコピーする `return e` を「`settleResolve(state_ref.__async_promise, e)` + 素の `return`」 に書き換える walker (`rewrite_returns_in_*`、 FnExpr には降りない — closure の return は closure のもの)。 tail がそのまま `return e` の場合の二重 settle は `emit_settle` の root-Return 特例で回避。 await 前 / await 間 (取る側・取らない側) / churn を確認。

**第 11 弾 sweep の取りこぼしも 1 件修正**: `return_sweep_base` — 引数なし fn は param scope が存在せず**本体が scope 0** になるため、 固定 `skip(1)` が全束縛を取りこぼしていた (deinit が早期 return で発火しない形で発見)。 `lower_block_for_fn_body` が本体直前の `env.scopes.len()` を記録し sweep の基点にする。 `__main` は usize::MAX (top-level return がもし通っても slot 管理と二重解放しない)。 deinit は sweep で内側束縛から順に発火することを pin。

**probe で問題なしを確認した周辺** (再調査不要): for-in 内の continue / break / return (heap local 跨ぎ)、 closure 本体の早期 return (lower_block_for_fn_body 経由で base 設定済み)、 break の既存 sweep、 async 早期 return の resolve 経路回帰。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 回帰 fixture 2 件: `05_edge_cases/early_exit_sweep_continue_scrutinee.il`、 `04_modules/async_early_return.il`。

### [解決済み記録] 第 11 弾: await の rejection propagate (新意味論) と早期 return の scope release 欠落 (2026-06-11)

probe 対象を await × rejected promise、 `Promise.reject` factory churn、 fire 中の再入登録、 継続内からの連鎖 settle、 1000 段 then 連鎖、 async method churn、 入れ子 promise / tuple payload / `Promise<()>` / `Optional<Promise>` field / enum-holds-promise へ広げた。 成果は新意味論 1 件 + 一般バグ 1 件:

1. **await が rejected promise に当たったら async fn の結果 promise を同じ msg で reject する (JS 意味論、 ユーザー決定)**。 旧挙動は rejection が無言で消えて結果 promise が永久 pending (`.catch` 不発)。 実装: runtime に `$promise.rejectFollows(upstream, target)` (upstream の reject を target へ転送する forwarder — `promise_reject_stub` cell を再利用、 resolve は無視)、 desugar の suspend が `__awaited` を一度束縛して `.then(resume)` と `rejectFollows(__awaited, state_ref.__async_promise)` の両方を登録 ([gen_items.rs](../crates/ilang-parser/src/normalize/state_machine/gen_items.rs))。 checker の内部 static (`$promise.rejectFollows<T, U>`) と codegen 配線を追加。 docs (syntax.md / syntax_ja.md の async 節) に明記。 fixture: `04_modules/async_await_rejection_propagates.il` (同期 reject / timer deferred / 2 段 async 入れ子 / ループ内 / churn、 400 並列で出力 1 パターン確認済み)。
2. **早期 `return` が生きている heap 束縛を release しない** (既存・一般)。 `lower_break` には早期脱出 sweep があるのに `lower_return` には**何も無く**、 fn 直下 / match arm / loop body どこでも「`return` を跨ぐ heap `let`」が 1 個/call leak していた (tail 形は正常)。 async では poll fn の suspend が生成する `return` がこれを踏み、 awaited promise + state に乗る全ローカルが leak (rejection 経路で顕在化したが resolve 経路でも同じ — ManagedPromise が untracked なため不可視だった)。 修正: [control.rs::lower_return](../crates/ilang-mir/src/lower/control.rs) に「param scope (借用) を除く全 scope の sweep」を追加し、 per-binding 規則 (CRepr 所有 / COM 除外 / PatternBinding 除外 / Cell) は `release_top_scope_objects` と共通の helper に括り出し。 借用値の `return` は sweep 前に Retain (coerce が新しい値を mint した場合は除く — `T → T?` wrap は coerce 側が所有を作る)。 fixture: `05_edge_cases/early_return_scope_release.il` (3 形の churn + 借用 return の UAF ガード)。

**自分の fixture でタイミング依存を踏んだ記録**: 初版の fixture が「5ms timer の発火」と「churn 区間」の前後関係に依存し、 nested_generic 800 連発との並走負荷で suite を 1 回落とした (`target/fixture-failures.log` に記録)。 規約の再確認 — timer 発火位置を固定するには **`time.sleep(期限+余裕)` で期限を確実に跨いでから `time.tick()`** (due は時刻基準なので負荷は「より due」方向にしか働かない)。 churn の定数残差 (state machine 初回確保) は exact 値でなく `expectTrue(|delta| < 1024)` で判定する。

**probe で問題なしを確認した周辺** (再調査不要): `Promise.reject` factory churn (catch あり)、 fire 中の同一 upstream への再入 `.then` 登録、 継続内からの別 deferred resolve (drain 中の連鎖 settle)、 1000 段 then 連鎖、 async method (heap field 越し await) churn、 `Promise<Promise<T>>`、 tuple payload churn、 `Promise<()>` 印字、 `Optional<Promise>` field churn、 enum variant (tuple / struct) が promise を保持する形。 REPL は `use` 自体が未対応 (`unexpected Item::Use post-loader`) — async と独立の既存制限として記録。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800、 新 fixture 2 件とも 400 並列で出力 1 パターン。

### [解決済み記録] fixture 増殖ラウンド第 9 弾: float executor の ABI 不一致・fresh 引数 release の残り 3 経路・TLS 破棄順 abort (2026-06-11)

probe 対象を float 型 promise、 rejection 経路の ARC churn、 同一 promise への複数 waiter、 heap 各種別の promise 値 churn、 暗黙 `this.method(...)`、 timer の相互 clear / executor 内 tick 再入 / float の Promise.all/race、 armed timer を残した panic 終了へ広げた。 4 系統のバグ (1 は新モデル起因、 3 は移行前から — 移行前バイナリで再現確認済み):

1. **float 型 promise の executor 経路が ABI 不一致で garbage / SIGSEGV** (既存)。 executor の `resolve` は ilang 側で `fn(f64)` として呼ばれ値が float レジスタに乗るが、 resolve_cb の fn_addr (Rust stub) は `(i64, i64)` で読む — 値に env が、 env に garbage が入り、 inline は garbage 値、 deferred は capture 読みで SIGSEGV。 float-ABI の stub 変種 (`promise_resolve_stub_f32/_f64`、 to_bits で `State::Resolved` の bits 規約に合流) を追加し、 `$promise.withExecutor` に `value_fk` 引数を追加 (lowering は `MirTy::F32/F64` から算出、 [program_decl.rs](../crates/ilang-mir-codegen/src/compile/program_decl.rs) は ternary 宣言へ)。 fixture: `04_modules/promise_float_executor.il`。
2. **fresh 引数の post-release が「クロージャ (間接) 呼び出し」に無い** (既存)。 named-fn 経路 ([call_fn.rs](../crates/ilang-mir/src/lower/call_fn.rs)) にはあった「fresh heap 引数の +1 を呼び出し後に release」が CallIndirect / CallRawIndirect 経路に無く、 closure へ渡した fresh string/object が 1 個/call leak。 promise の `resolve(new Box(..))` / `rej("...")` の値 leak (複数 waiter で 24 bytes/iter、 reject 文字列 1 個/iter など) は全部この現れ。 fixture: `10_closures_arc/closure_fresh_arg_release.il`。
3. **同じ release が「暗黙 `this.method(...)`」arm にも無い** (既存)。 fresh string は 1 個/call leak (fresh object は escape 解析の stack 化で隠れがち)。 さらに **promise `.then`/`.catch` の fresh receiver release も無く**、 チェーン `p.then(f).catch(g)` の中間 promise が settled 値ごと leak (ManagedPromise 自体は Box 確保で `liveAllocBytes` に映らず、 保持された rejection string が 1 個/iter で見えた)。 waiter は downstream を +1 保持するので fresh receiver の release でチェーンは切れない。 fixture: `02_classes/method_implicit_this_fresh_arg.il`、 `04_modules/promise_chain_intermediate_release.il`。
4. **armed timer を残した panic 終了で TLS 破棄順 abort** (新モデル起因)。 thread-local の `EventLoop` 破棄が残存 timer entry の closure release cascade を走らせ、 既に破棄された別 thread-local (cascade worklist / string registry) に触れて二重 panic → abort、 本来の panic メッセージが大量のノイズに埋まる。 `EventLoop` の `Drop` で残存タスクを `mem::forget` (プロセス終了中なので OS が回収)。 panic 終了は exit 1 + 本来のメッセージのみになった (fixture 化は困難 — expect-error はノイズの「不在」を表明できないため手動確認、 再発時はこの記録を参照)。

**probe 計測の罠 (保管用)**: `console.log("a=" + x + " b=" + (test.liveStringCount() - base))` のように **count 読みを concat 式の途中に置くと、 左半分の concat 中間文字列が生きたまま count が走って幻の +1** が出る。 churn probe では必ず値をローカルへ読み切ってから印字する。 closure 内での `test.expectTrue` 初回呼び出しも定数 +1 を作る — 検証は計測ループの外で行う。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 (初回の workspace 実行で 1 件 leaky 表示 — テスト自体は PASS、 nested_generic 800 連発と並走させた負荷下のみで観測され、 単独再実行では 0 leaky。)

**probe で問題なしを確認した周辺** (再調査不要): rejection の then 素通り→catch 連鎖と uncaught reject の msg 収支 (修正後 delta=0)、 同一 promise 3 waiter / heap 各種別 (string・配列・Optional) の executor→then churn、 then が heap 値を返して downstream を捨てる形、 timer A→B 相互 clear と callback 内再スケジュール、 executor 本体からの `time.tick()` 再入、 float の Promise.all / race (bits 規約で整合)、 巨大 ms。

### [解決済み記録] fixture 増殖ラウンド第 8 弾: executor 3 セル leak と ICF 重複登録の二重解放 (2026-06-11)

probe 対象を新実行モデルの周辺 — timer の ARC churn (発火済み / cancel 済み / interval 自己 clear)、 deferred パターン (executor から resolve/reject を持ち出す JS `Promise.withResolvers` 相当)、 発火順序 (同一期限 FIFO / microtask vs macrotask / 入れ子 0ms 連鎖)、 pump/drain の再入 (callback 内 `time.tick()` / callback 内 `liveAllocBytes`)、 timer 解決の promise と await / Promise.all / catch の統合、 巨大 ms / 二重 clear / 未知 id — へ広げた。 2 系統のバグ (どちらも**移行前から存在**、 移行前バイナリで再現確認済み):

1. **`new Promise<T>(executor)` が 3 セルとも leak** (空 executor で正確に 16+32+24=72 bytes/回 + capture)。 lowering ([crates/ilang-mir/src/lower/expr.rs](crates/ilang-mir/src/lower/expr.rs) の Promise arm) は executor の +1 を runtime へ譲渡する規約 (fresh はそのまま、 非 fresh は Retain) だが、 `__promise_with_executor` が譲渡分を release していなかった。 runtime が rc=1 で mint する resolve_cb / reject_cb も「executor 本体が param を release する」という旧コメントが現行 MIR では成り立っておらず未解放。 修正: 同期 executor 呼び出しの直後に 3 つとも release。 escape された callback は容器 store の retain で生存する (deferred パターンは正常動作)。
2. **release ビルドの関数マージ (ICF) で capture 登録が二重化 → promise の二重解放** (`promise_stub_merged_registration.il` の形で `.catch` が無言で不発 / 配置次第で SIGABRT)。 `promise_resolve_stub` と `promise_race_resolve_stub` (reject 側も) は本体が同一で release ビルドでは同一アドレスに統合される。 capture 登録テーブル ([crates/ilang-runtime/src/closures.rs](crates/ilang-runtime/src/closures.rs)) は fn_addr キーに blind push なので、 `Promise.all`/`race` の stub 登録が executor stub と同じキーへ `(+16, KIND_PROMISE)` を重複追加 → 以後の callback cell 解放で captured promise が 2 回 release → 早期 FINAL-DROP で waiter ごと消える。 **バグ 1 の修正で cell が実際に解放されるようになって顕在化した潜在バグ** (leak していた間は cascade が走らなかった)。 修正: `__register_closure_capture` を (offset, kind) で冪等に。 マージされる関数はコードが同一 = capture 配置も同一なので dedup で失うものはない。

**デバッグ補助を常設化**: `ILANG_DEBUG_PROMISE=1` (promise の retain/release/FINAL-DROP/settle)、 `ILANG_DEBUG_CLOSURE=1` (closure cell の rc 遷移)、 `ILANG_DEBUG_TIMER=1` (timer の schedule/fire/discard)。 env 判定は OnceLock キャッシュで hot path は atomic load 1 回。 今回の真因特定はこの 3 つの突き合わせで行った。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 回帰 fixture 4 件: `04_modules/promise_executor_cell_arc.il` (churn 4 変種 delta=0)、 `04_modules/promise_deferred_resolvers.il` (deferred + await/all/catch の timer 解決)、 `04_modules/promise_stub_merged_registration.il` (all/race を先に走らせてから deferred catch — ICF 二重登録の pin)、 `04_modules/timer_microtask_order.il` (同一期限 FIFO / then-before-timer0 / 0ms 連鎖 / tick 再入)。

**probe で問題なしを確認した周辺** (再調査不要): 発火済み・cancel 済み setTimeout / 自己 clear interval の heap capture churn (delta=0)、 巨大 ms (`i64::MAX`) の schedule + clear (overflow せず即終了)、 二重 clear / 未知 id clear / 発火済み id clear、 callback 内 `time.tick()` 再入・callback 内 `liveAllocBytes` (pump) 再入、 同一期限 3 本の FIFO、 queue 済み継続が due timer より先 (JS microtask 規約)、 入れ子 0ms timer 連鎖が単一 tick 内で 5 段、 timer 解決 promise の await / Promise.all (逆順解決) / race / escape された reject の catch 復帰。

### [解決済み記録] Promise/async 実行モデルを JS 型 (run-to-completion・シングルスレッド) へ移行した (2026-06-11)

ユーザー決定 (2026-06-11) を実装。 動機: 旧モデルは executor / `.then` 継続 / await 再開が work-stealing worker pool 上で**メインスレッドと並行に**走り、 (a) 同期コードからの promise 状態観測が非決定的 (`promise_print_state.il` の race を高負荷 1/400 で実測 — 下の記録)、 (b) callback がメインと同じ容器を触るとデータ競合になり得た (同期プリミティブが無いため防げない)。 実装計画は `/Users/iwao/.claude/plans/tingly-puzzling-dusk.md` (保管用)。

**新意味論** (JS / Node と同じ):

- ユーザーコード (main・executor・`.then` callback・async fn の再開・タイマー callback) は**すべて単一スレッド**で run-to-completion。 同期コード実行中に継続が割り込むことはない
- executor は `new Promise(...)` の**その場で同期実行**。 resolve/reject は状態遷移 + 継続の queue 投入のみ (inline では走らせない)
- 継続はメインスレッドの FIFO queue。 実行は drain ポイントのみ: プログラム終了時 (`run_main` / AOT main wrapper の `$promise.drain`) と、 新設の **`time.tick()`** (非ブロッキング pump — 期限到来済みタイマー + queue を空にして即 return。 自前メインループを持つアプリがフレームごとに呼ぶ)
- タイマーは [crates/ilang-runtime/src/pool.rs](crates/ilang-runtime/src/pool.rs) の期限順 BinaryHeap。 終了時 drain は期限まで sleep して発火させる (未発火タイマーはプロセスを生かす — Node と同じ)。 cancel 済み entry は先頭に来た時点で破棄するので、 cancel された長期タイマーが終了を遅らせることはない

**実装**: `pool.rs` を worker スレッド機構ごと書き換え (`submit`/`drain` の API 維持、 `schedule_timer`/`cancel_timer`/`pump` 追加、 thread-local の queue + timer heap + live map。 crossbeam-deque 依存を撤去)。 `promises.rs::__promise_with_executor` を同期呼び出し化 (worker 用の retain/release を撤去 — caller の borrow が同期呼び出しを跨いで生きるため不要)。 `timers.rs` は「pool に sleep 入りタスクを submit」から timer heap 登録へ (Mutex/Arc/AtomicBool 撤去)。 `$time.tick` export + jit_symbols 配線 + `libs/std/time.il` の `pub fn tick()`。 将来の CPU 並列は「共有メモリなしの worker API」(別設計) で提供する方針。

**ハマりどころ (保管用)**: timer callback の +1 を Drop guard (`ClosureGuard`) で持たせる実装で、 `move ||` クロージャ内の利用が `guard.0` (Copy な i64 フィールド) だけだと **Rust 2021 の disjoint capture がフィールドだけを capture して guard 本体を即 drop** → callback cell が発火前に解放され、 `fn_addr=0` の無言スキップになる (クラッシュせず「タイマーが発火しない」だけなので気づきにくい)。 メソッド呼び出し (`guard.ptr()`) にして全体 capture を強制した。

**fixture の書き方が変わる点**: run-to-completion では main 末尾の検証コードがタイマー/継続より**先に**走る。 タイマー発火を検証する fixture は「callback 自身が検証して印字する」JS 流に書く (`timer_set_interval_clear.il` を 3 tick で自己 clear する形に書き換え。 旧形は `time.sleep(300)` 中に worker が発火する前提だった)。

**後続修正 (同 2026-06-11)**: `test.liveAllocBytes` / `liveAllocCount` 内の flush を `drain` から **`pump` (非ブロッキング)** に変更。 drain はタイマー heap が空になるまで待つため、 interval が armed なまま probe を呼ぶと永久ブロックしていた (旧 worker pool モデルでも同形でハングする既存の角 — interval は終わらない pool タスクだった)。 probe の目的は「queue 済み継続を流しきって callback 待ちの確保分を leak と誤計上しない」ことなので pump で足りる。 fixture: `05_edge_cases/live_alloc_probe_nonblocking.il` (armed interval + probe が即返る + 自己 clear で終了も pin)。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 `promise_print_state.il` / `promise_race_array_order_tiebreak.il` を 16 並列 × 25 batch (各 400 回) で出力 1 パターン (決定性の実証)、 nested_generic.il 16×25×2 = **0/800**。 `examples/libs/gui/window_state_demo` はビルド確認済み (GUI 起動はしていない)。 docs: syntax.md / syntax_ja.md の Promise 節を新実行モデルに書き換え。

**持ち越し (任意のクリーンアップ)**: 同期プリミティブの簡素化 — `ManagedPromise` の `rc: AtomicI64` → i64、 `inner: Mutex<Inner>` → RefCell、 `PromiseAllState.remaining` の Atomic 外し、 `closures.rs` / `strings.rs` 等の atomic rc はシングルスレッド化後は過剰。 機能とは独立の commit で行うこと (今回は未着手)。

### [解決済み記録] AOT arm の確率的失敗は `promise_print_state.il` のタイミング依存期待だった (2026-06-11、 `df797334`)

**真因 (再現により確定)**: 第 4 弾で追加した [crates/ilang-cli/tests/programs/04_modules/promise_print_state.il](crates/ilang-cli/tests/programs/04_modules/promise_print_state.il) が `Promise.all([Promise.resolve(1)])` を直後に印字して `<promise pending>` を期待していたが、 `Promise.all` の解決継続は worker pool (実 OS スレッド) 上で**メインスレッドと並行に**走る。 アイドル機ではメインの print が常に勝つが、 高負荷時は pool が勝って `<promise resolved>` が印字され expect 不一致 → fixture FAIL → suite FAIL。 **16 並列 × 25 batch で 1/400 を実測再現**。 全観測と整合: 自然発生 2 回はどちらも「リビルド直後でマシンが高負荷」/ この fixture の追加後 / 記録罠の設置前。 アイドル機での意図的再現 27 回が全 PASS だったのは負荷条件を満たしていなかったため。

**修正**: pending の印字確認は「executor が resolve を呼ばない永続 pending の promise」に差し替え (同じ 400 並列で 400/400 決定的)。 `Promise.all` は `.then` での値検証に変更。

**二次容疑は白**: abort 系 expect-error fixture (`repr_c_enum_field_unknown_aborts.il`) の「abort 前 stderr 喪失」は同じ並列負荷 208 回で 0 回 — 現行の abort 経路では起きない。

**維持するもの**: 第 5 弾の harness hardening (`39d87ad7` — spawn 失敗 / リンク直後 SIGKILL(9) の 1 回 retry + `target/fixture-failures.log` への自動記録、 fixture 失敗詳細の同ファイル記録) は環境起因の防御としてそのまま残す。 タイミング依存の期待を持つ fixture を書かないこと (`<promise pending>` のような並行状態の印字は「決して解決しない promise」で固定する)。

### [解決済み記録] fixture 増殖ラウンド第 7 弾: for-in の live 化・分解束縛の ARC・テンプレート part 入力 (2026-06-11、 `ced57791`〜`52c6bb8f`)

probe 対象を for-in 中の mutation、 tuple / struct 分解束縛、 Result の heap 両側、 float キー Map、 入れ子テンプレート、 events モジュール churn へ拡張した。 3 系統のバグ:

1. **for-in 中の pop で「index out of bounds」panic** (`ced57791`)。 配列 arm が `ArrayLen` をループ前に巻き上げており、 本体内の pop で stale な長さのまま境界検査に当たっていた。 長さをループ header で毎周読み直す live 意味論 (JS の for..of と同じ) に変更: pop は途中終了、 push は追加要素も巡回。 ArrayLoad は data pointer を毎アクセス読み直すので push の realloc はもともと安全。 fixture: `05_edge_cases/forin_live_mutation.il`。
2. **分解束縛の ARC 2 点** (`8185589c`)。 `let (a, b) = makePair()` が fresh tuple を release せず 32 bytes/回 leak。 さらに束縛が要素を retain しないため、 **借用 tuple の分解**では束縛の scope-exit release が tuple の slot share を横取りし、 tuple の後続 cascade が解放済みセルを歩いていた (解放済みセルの rc がちょうど 1 に読めることが稀なため显在化していなかっただけ)。 束縛ごとに retain + fresh source は抽出後に release。 `let Class { f } = obj` (LetStruct) も同型で同修正。 fixture: `05_edge_cases/destructure_arc.il`。
3. **テンプレート `${}` 内の fresh 値が 1 個/評価 leak** (`52c6bb8f`)。 第 4 弾の修正は fmt 結果と concat 中間値だけで、 **part の入力** (`${`inner${x}`}` の内側テンプレートや `${"a" + b}` の concat) の transient +1 が残っていた。 fmt_value (Str 入力はコピー) の後に release。 `leak_template_literal_loop.il` に追記。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 400 並列 0/400。

**probe で問題なしを確認した周辺** (再調査不要): Result の heap 両側 (ok/err どちらも class) churn、 events モジュール (on/off/emit/removeAllListeners) churn、 float キー Map / Set (型検査で適切に拒否 — `Map<f64, _>` は診断あり)、 入れ子テンプレートの値の正しさ。

### [解決済み記録] fixture 増殖ラウンド第 6 弾: is/as? の ARC・reflection の kind tag・iteration snapshot (2026-06-11、 `e83ca1fc`〜`3fbf7667`)

probe 対象を `is` / `as?`、 reflection (`typeof`)、 regex、 iteration 中の mutation、 generic fn + heap、 Promise.all/race、 const 畳み込み、 string メソッドの端へ拡張した。 3 系統のバグ:

1. **`as?` の Optional セルが不正な 8 byte 形** (`e83ca1fc`)。 `DowncastOrNone` codegen が rc も kind tag も無い `[value]` 8 byte セルを確保 — 誰も release できず 8 bytes/hit leak、 release されたら範囲外読み、 inner の retain も無し。 WeakUpgrade と同じ 24 byte `[value | rc | kind=Object]` + inner retain に変更し、 `as?` を fresh 分類。 `is` / `as?` の fresh operand (`makeB() is B`) の transient release も追加 (旧: オブジェクト丸ごと leak/call)。
2. **reflection の ARC 3 点** (`3437012f`)。 (a) `new_string_array` の header kind tag が PK_STR(11) — release cascade の KIND_* 系では **11 = WEAK** で、 `.fields` / `.methods` の要素文字列の release が weak-table no-op になり 1 個/読み leak。 KIND_STR(7) に修正。 (b) `__type_parent` の Optional セルが中身 = TypeHandle id (整数) なのに kind=Object — release されたら整数に `__release_object` して暴走する形。 KIND_NONE に修正。 (c) reflection member 読み (`t.name` / `t.fields` 等、 全部 owned を返す) が借用分類で leak — `field_is_property_access` が TypeHandle member 読みを fresh と認識するよう拡張 (`.kind` の interned enum box は rc=-1 で release no-op なので一律 fresh で安全)。
3. **callback 駆動 iteration の use-after-free** (`3fbf7667`)。 配列の forEach / map / filter / find / findIndex / every / some が `(len, data)` を 1 回だけ読んで生バッファを歩く — callback 内の push (realloc) / pop で**解放済みメモリを走査** (`arr.forEach` 内の `arr.push` で walk が途中で止まる)。 共通 `CellSnapshot` (セルを先にコピー + heap 要素は iteration 中 +1 保持、 `__map_for_each` と同じ規約) に統一。 Map の forEach も key しか retain しておらず、 callback 内 delete で snapshot の **value** が解放されて読みが実行ごとに揺れていた — value も retain。 さらに `lower_index` の fresh receiver 経路が Object 要素限定で release していたため、 `freshStrArray()[0]` で配列丸ごと leak — 全要素種で release + `is_arc_slot` 要素は retain に一般化。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 400 並列 0/400。 回帰 fixture 3 件: `05_edge_cases/downcast_arc.il`、 `02_classes/typeof_reflection_arc.il`、 `05_edge_cases/iter_mutation_snapshot.il`。

**probe で問題なしを確認した周辺** (再調査不要): regex (compile/test/find/findAll/split/replace churn)、 generic fn + heap 引数/戻り churn、 Promise.all / race (型注釈付き — 注釈なしは既知の desugar 制限で適切な診断あり)、 const 畳み込み (連鎖参照 ✓、 0 除算は実行時 panic に遅延)、 string split/indexOf/lastIndexOf/slice/charAt の端、 Set forEach (element retain 実装済み)、 `i64::MIN / -1` 等の数値の罠 (第 5 弾で確認済み)。

### [解決済み記録] fn 本体内の自己再帰 closure を `ClosureSelf` で対応 (2026-06-11、 `70b6dde8`)

**旧状態**: `let fib: fn(i64): i64 = fn(n) { ... fib(...) }` はトップレベル (slot 経由の遅延解決) のみ対応。 fn 本体内では型注釈があっても `undefined function` で型検査落ち。 capture では原理的に解決できない (構築前の値の snapshot になるか、 cell 経由だと closure ↔ cell の retain 循環になる)。

**実装**: 新 MIR 命令 `Inst::ClosureSelf` = 隠し末尾 env パラメータ (= 実行中の closure 自身) を実体化する。 closure 本体内の自分の束縛名への参照 (呼び出し・値利用) はこれに解決される — capture なし・循環なし・escape しても「参照」が closure 自身に同行するので安全。 入れ子 closure が外側 closure の名前を参照する場合は ClosureSelf の値 snapshot を retain 付きで capture (外側は内側を保持しないので循環でない)。 tail で自分自身を返す場合は borrow 扱いで retain。 型検査は注釈付き fn 型を RHS 検査用 env に事前束縛 ([crates/ilang-types/src/checker/stmt.rs](crates/ilang-types/src/checker/stmt.rs))。 注釈は必須 (再帰型は推論不能) で、 注釈なしは `SelfRecursiveClosureNeedsAnnotation` の専用診断 (旧: 裸の undefined function)。 トップレベルの slot 遅延解決は従来どおり。

**検証**: workspace nextest 530/530、 AOT arm PASS、 nested_generic.il 400 並列 0/400。 fixture: `10_closures_arc/closure_self_recursion_fn_body.il` (素 / capture 併用 / escape / 入れ子 / 200 周 churn)、 `closure_self_recursion_unannotated_error.il`。 docs: syntax.md / syntax_ja.md のクロージャ節に追記。 ilang-lsp release ビルド済み。

### [解決済み記録] fixture 増殖ラウンド第 5 弾: 深い解放 cascade の stack overflow と REPL の chunk 跨ぎ解放 (2026-06-11、 `6c5437dd` + `80736da5`)

probe 対象を数値演算の罠 (`i64::MIN / -1`・shift 64・float→int の NaN/inf)、 深い連結構造の解放、 自己再帰 closure、 range の端、 対話 REPL へ拡張した。 2 件の重大バグ:

1. **~10 万リンクの連結構造を解放すると native stack overflow** (`6c5437dd`)。 [crates/ilang-runtime/src/cascade.rs::release_field_by_kind](crates/ilang-runtime/src/cascade.rs) が object → optional → object と 1 リンクごとに再帰していた。 入れ子の release を thread-local の worklist に積んで最外フレームだけが drain する反復方式に変更 — 呼び出し深さはグラフ深さに依らず O(1)。 解放順は深さ優先から幅優先寄りに変わるが、 ARC は兄弟間の解放順を保証しないし、 deinit から届く値は未解放のまま (保守方向)。 deinit は 1 ノード 1 回 (fixture の 5 万連鎖カウンタで pin)。 fixture: `05_edge_cases/deep_release_iterative.il` (Optional 連結 10 万 / enum cons 連鎖 10 万 / deinit カウント 5 万)。
2. **対話 REPL で heap 値の slot が chunk 終了ごとに解放され、 次の行が解放済みメモリを読む** (`80736da5`)。 __main のエピローグ (slot 一掃 release — ファイル実行では「終了前に deinit を発火」として正しい) が REPL の chunk ごとに走っていた。 `let arr = [1,2,3]` の次の行で `arr[1]` が無言の SIGSEGV、 `arr.length` が 0、 object の field 読みが garbage、 Map 読みが死ぬ。 `lower_program_with_slots_opts(.., release_slots_at_exit)` を追加し REPL は false。 同エピローグの Cell arm が「cell の中身だけ release」する旧規約のままだった点も第 2 弾のモデルに統一。 回帰テスト: `crates/ilang-cli/tests/repl.rs` (セッションをパイプで流して primitive / array / object / map / string の chunk 跨ぎを pin、 nextest 対象に +5 件)。

**問題なしを確認した周辺** (再調査不要): `i64::MIN / -1` (= MIN、 trap せず) と `% -1` (= 0)、 整数 0 除算の panic 診断、 `<< 63/64`、 float→int の飽和 (`1e300 as i64` = MAX、 NaN = 0)、 range の端 (`5..2` / `3..3` 空、 `0..=0` 1 回、 負範囲)、 配列リテラルの固定長型と `push` の型エラー (run / REPL 一致)。

### [解決済み記録] fixture 増殖ラウンド第 4 弾: async desugar の await 位置と Promise 印字 (2026-06-10 後続セッション、 `87a8ea2a` + `65ff1e4c`)

probe 対象を AOT 一斉実行 (`ILANG_TEST_AOT=1` 全 fixture)、 実行中 closure の自己差し替え、 interface dispatch churn、 set 演算の object 要素、 async の深掘り、 match/if-let payload の escape、 overload + heap 引数へ拡張した。 async だけが 2 系統で崩れていた:

1. **代入文の RHS / 入れ子ブロック内の await でコンパイラ panic または誤診断** (`87a8ea2a`)。 await-lift 前処理 ([crates/ilang-parser/src/normalize/async_desugar/await_lift.rs](crates/ilang-parser/src/normalize/async_desugar/await_lift.rs)) に Assign / AssignField / AssignIndex の arm がなく、 while / loop / for-in 本体・if 枝・match 腕・素の block へも降りていなかった。 `total = total + await p` は文位置だと `unreachable!("NoAwait after body_contains_await=true")` の **コンパイラ panic**、 ループ内だと「await は async fn 内のみ」という誤った診断になっていた。 lift を代入系 + 入れ子ブロックへ再帰させ (持ち上げた `let __await_tN` は見つけた block 内に留めて反復/条件性を保存)、 持ち上げ不能な位置 (`&&`/`||` の右辺等) は unreachable ではなく正規の診断にした。
2. **Promise 値の print / format が生ポインタ** (`65ff1e4c`)。 `console.log(p)` と `${p}` が raw-int fallback に落ちていた。 `$print.promise` / `$fmt.promise` を追加して `<promise pending|resolved|rejected>` を出すようにした。

**検証**: workspace nextest 525/525、 AOT arm 全 fixture PASS (1342 runs)、 nested_generic.il 16×25×2 = 0/800。 fixture 3 件: `04_modules/async_await_in_assignment.il`、 `04_modules/async_await_logical_rhs_error.il`、 `04_modules/promise_print_state.il`。

**probe で問題なしを確認した周辺** (再調査不要): AOT arm の全 fixture (今日の ARC 修正一式を含む)、 実行中 closure が自分を保持する field を差し替える形 (旧 closure は呼び出し終了まで生存 ✓)、 interface (vtable) dispatch + interface 型配列の churn、 object 要素の set union / intersection / difference churn、 async class method + await チェーン (P4 検証値一致)、 match / if-let の payload を容器へ escape、 heap 引数の overload dispatch。

### [解決済み記録] fixture 増殖ラウンド第 3 弾: heap 値 property の ARC を owned 規約に統一 (2026-06-10 後続セッション、 `df0e3f16`)

probe 対象を deinit 連鎖の周辺 (deinit 中の field 読み・メソッド呼び・コンテナ経由の連鎖発火)、 async churn、 generic class + heap field、 static field、 Unicode 文字列、 heap 値 property へ拡張した。 property だけが 3 点で崩れていた:

1. **setter への fresh 値が 1 個/call leak** — instance / static とも setter 呼び出し経路 ([crates/ilang-mir/src/lower/expr.rs](crates/ilang-mir/src/lower/expr.rs) の AssignField property 分岐) が transient +1 を release していなかった (setter 本体の store は AssignField 経由で自前 retain を取る)。
2. **`{ this.inner }` (Field tail) の getter が +1 を返すのに消費側は借用扱い** — 旧 `is_property_getter` の除外は bare-var tail だけに効いており、 Field/Index tail の getter は tail_is_borrow の無条件 retain で +1 を返していた。 entry が上書きされた時点で 1 share が永久に残る (`__map_get` と同型)。
3. **fresh tail の computed getter (`get fresh(): Box { new Box(..) }`) は読みごとに 1 個 leak** — 借用規約では mint された値の所有者が存在しないため、 構造的に直せない。

**修正 (owned 規約へ統一)**: getter は常に +1 を返す (bare-var tail の `is_property_getter` 除外を撤去 — フィールド/メソッドと同じ tail retain 規則に一本化し、 フラグ自体を削除)。 消費側は `is_fresh_object_expr` の `Field` arm で receiver の静的型が構文的に解決できる場合 (`Var` 束縛 / repl slot / `this` / クラス名 = static property) に `obj.prop` を fresh と分類 ([body_cx.rs::field_is_property_access](crates/ilang-mir/src/lower/body_cx.rs)、 lowering の dispatch と同じ property_getter map を参照)。 setter は instance / static とも fresh 値を呼び出し後に release。

**既知の制限**: receiver の静的型を構文的に解決できない形 (`call().prop`、 `arr[0].prop` 等) は借用 fallback のままなので、 その読みは getter の +1 が leak する (use-after-free にはならない)。 is_fresh が lowering 後の型を見られるようになったら解消できる。

**検証**: workspace nextest 525/525、 nested_generic.il 16×25×2 = 0/800。 回帰 fixture: `08_properties/property_heap_value_arc.il`。 **probe で問題なしを確認した周辺** (再調査不要): deinit 連鎖中の自 field 読み / メソッド呼び / tally 書き込み、 Map overwrite / array pop によるコンテナ経由の連鎖 deinit 発火回数、 async fn churn (`await` ループ 200 周 delta=32 定数)、 generic class (string / array / nested Box) churn、 static field 増分 churn、 Unicode 文字列メソッド (length / charAt / slice / split / indexOf)。

### [解決済み記録] 継承時の `deinit` を Swift と同様の自動連鎖に変更 (2026-06-10 後続セッション、 `37a4e8b9`)

**旧挙動**: `class Derived: Base` で両方に `deinit` がある場合、 派生側の deinit が親側を**置き換え**、 親の後始末は走らなかった (明示の `super.deinit()` が連鎖手段だった — 旧 `inheritance_deinit_chain.il` がその挙動を pin していた)。

**新仕様 (ユーザー判断)**: Swift と同様の自動連鎖。 破棄時に最派生クラスの deinit → 各祖先の deinit の順で自動実行。 deinit を持たないクラスは連鎖を切らずにスキップ。 `super.deinit()` は自動連鎖と二重実行になるため、 他の明示 deinit 呼び出しと同じく型検査で拒否 (`CannotCallDeinit`)。

**実装**: [crates/ilang-mir/src/lower/decl/class.rs](crates/ilang-mir/src/lower/decl/class.rs) の class decl で、 自前 deinit + 祖先 deinit の両方があるクラスに `Class.deinit$chain` wrapper (自前 deinit を呼んでから直近祖先の drop_fn を呼ぶ合成 MIR fn) を生成して `drop_fn` に据える。 祖先側の drop_fn は処理順 (親が先) により既に連鎖済みなので再帰的に根まで届く。 deinit 本体内の早期 `return` でも連鎖が切れないよう、 tail への呼び出し追記ではなく wrapper 方式を採用。 `super.deinit()` の拒否は [crates/ilang-types/src/checker/expr/mod.rs](crates/ilang-types/src/checker/expr/mod.rs) の SuperCall arm。

**検証**: workspace nextest 525/525、 nested_generic.il 16×25×2 = 0/800。 fixture: `02_classes/inheritance_deinit_chain.il` (新仕様に書き換え)、 `02_classes/inheritance_super_deinit_error.il` (拒否の pin)、 `09_subtyping/deinit_chain_to_parent.il` (Base 型束縛経由・deinit なし中間クラス・早期 return・200 周 churn)。 docs: syntax.md / syntax_ja.md のクラス基本節と継承節に連鎖規則を明記。 ilang-lsp も release ビルド済み。

### [解決済み記録] fixture 増殖ラウンド第 2 弾で検出した 3 系統 (2026-06-10 後続セッション、 `7ba1d28f`〜`9912a69c`)

probe 対象を可変ローカル / top-level slot / field / enum / Optional の上書き churn、 配列の fill / concat / slice / reverse / removeAt / unshift / sort、 cell capture (閉包が捕獲変数へ代入)、 break 値、 再帰、 vtable、 継承解放へ拡張した。 検出と修正:

1. **cell capture (mutating capture) が cell + 中身を毎構築 leak** (`7ba1d28f`)。 capture 登録が `is_cell` を「leak for now」でスキップしており、 `__release_closure` の cascade が cell を解放できなかった (1 閉包あたり 56〜80 bytes)。 所有権モデルを「scope が生成 +1 を持ち scope exit で release / 各閉包が MakeClosure で +1 retain し cascade で release」に統一。 落とし穴 2 つ: (a) 外側閉包の env から転送する cell ポインタが `MirTy::I64` で型付けされていて retain が無効化 → cascade だけ効いて過剰解放 (`closure_nested_shared_cell.il` の index OOB)。 cell 配列型で型付けして解決。 (b) その場で mint する private snapshot cell は閉包が生成 +1 を引き取る (retain しない)。 break の早期脱出 sweep も「中身を release」から「cell を release」に変更 (旧挙動は生存閉包に対して slot を dangle させていた)。
2. **`arr.fill(v)` の rc 2 点** (`f7bce86c`)。 lowering が fresh な fill 値の transient +1 を release せず 1 個/call leak。 runtime が「旧値 release → 新値 retain」の順だったため、 自己 fill (`arr.fill(arr[0])`) で自分の slot の値を retain 前に解放しうる。 retain-first に変更。
3. **`loop { break v }` の評価値が borrowed 分類で 1 個/評価 leak** (`7ba1d28f` に同梱、 fixture は `9912a69c`)。 `lower_break` は borrowed break 値に Retain を積むので loop の評価値は常に +1 所有 — `ExprKind::Loop` を fresh whitelist に追加。

**検証**: workspace nextest 525/525 (新 fixture 3 件込み)、 nested_generic.il 16×25×2 = 0/800。 probe: cell capture 全形 (未呼び出し / escape / 兄弟 2 閉包共有 / 入れ子転送) delta=0、 fill churn + 自己 fill ✓、 break 値 churn delta=0。

**回帰 fixture 3 件**: `10_closures_arc/cell_capture_release.il`、 `03_collections/array_fill_arc.il`、 `05_edge_cases/loop_break_value_ownership.il`

**probe で問題なしを確認した周辺** (再調査不要): 可変ローカル / top-level slot / field / enum / Optional / weak field の上書き churn、 concat / slice / reverse / removeAt / unshift / sort (object 要素)、 string メソッド群 + `+=` churn、 Result churn、 for-in churn、 閉包配列 churn、 再帰 (heap 引数/戻り)、 vtable churn、 派生クラスを Base 型束縛で解放 (動的 class_id で extra field も cascade ✓)、 Set values / Map entries-keys-forEach churn。

### [解決済み記録] fixture 増殖ラウンドで検出した 5 系統 (2026-06-10 後続セッション)

「さらに fixture を追加してバグをあぶり出す」ラウンド。 leak は `test.liveAllocBytes()` / `test.liveStringCount()` を 200 周ループで挟む probe、 別名健全性は「束縛 → 上書き / delete 後に読む」probe で網羅した。 検出と修正:

1. **`arc_peephole` の不健全な対消去** (`480ed47a`、 正しさバグ)。 `is_safe_to_cross` が「他値の Retain/Release は跨いで安全」としていたが、 コンテナの Release は cascade で候補値の指す先を解放しうる。 `retain v; release map; load_field v; release v` の対が消され、 `makeMap()["k"].n` / `makeArr()[1].n` の inline member 読みが解放済みメモリを返していた (束縛経由だと正常なので気づきにくい)。 他値の Release / WeakRelease を barrier に変更。 Retain は増やすだけなので跨ぎ可のまま。
2. **weak back-ref の cascade 中二重解放** (`d3b1d2cf`、 既存の潜在バグ)。 `__release_object` は strong rc を 0 にしてから field cascade を歩くが、 cascade が「親への `.weak` を持つ子」 を解放すると、 子の field release → `__release_weak(親)` が strong==0 を見て親本体を free → cascade 終了後に親自身の release 末尾がもう一度 free。 親→子配列 + 子→親 weak の定番形で解放順依存の SIGSEGV / malloc abort。 cascade 前に guard weak を 1 本取り、 cascade 後に返す形に変更 (free は `__release_weak` に一本化)。 `weak_parent_back_reference.il` が確率的にこの形を踏んでいた。
3. **配列 `indexOf` / `includes` / `remove` の string ポインタ比較** (`ebb95b4a`、 正しさバグ)。 生セル比較のため intern された literal 同士しか一致せず、 `["aa","bb"].indexOf("b" + "b")` が -1。 string kind の要素は内容比較に変更 (`==` の構造的等値に一致)。 object は参照等値のまま (`==` と一致)。 `remove` は needle ではなく**格納セル**の share を release するよう修正 (内容等値では別ポインタ)。
4. **fresh transient の release 漏れ 3 兄弟** (`8b9f8c31`)。 (a) `m.get/has/delete(new Key(1))` / `s.has/delete(...)` の fresh needle 引数 — lowering が `set`/`add` しか release していなかった。 24 bytes/call (object) または registry string 1 個/call の leak。 (b) fresh receiver のメソッド (`("v"+s).includes(..)` 等) — string/array/Optional は結果が receiver の格納物を share なしで借りることがない (pop/shift は移転、 find/unwrap は retain 済み) ので呼び出し後に blanket release。 (c) `.length` / `.isSome` / `.isNone` は field 経路なので別途 lower_field 側にも release を追加。
5. **template literal の評価ごと leak** (`aace25c7`)。 `lower_template` が `fmt_value` 結果と中間 `str_concat` 結果を release せず、 最終結果も borrowed 分類で誰も release しない — `${i}` 1 個につき registry string 2 個が永久に残っていた (console.log のテンプレも同様)。 中間値は消費した concat の直後に release、 `Template` を `is_fresh_object_expr` の whitelist に追加して消費側が結果を release。 空テンプレートは intern された "" を返してしまうので fresh copy を mint して契約を無条件化。

**検証**: workspace nextest 525/525 (新 fixture 8 件込み)、 nested_generic.il 16×25×2 = 0/800。 probe で fresh needle / fresh receiver / template churn / weak 解放順 すべて delta=0。

**回帰 fixture 8 件**: `05_edge_cases/fresh_receiver_inline_member_read.il`、 `05_edge_cases/weak_backref_cascade_release_order.il`、 `03_collections/array_indexof_string_content_eq.il`、 `03_collections/map_set_fresh_needle_arg_release.il`、 `05_edge_cases/leak_fresh_receiver_members.il`、 `05_edge_cases/leak_template_literal_loop.il`、 `03_collections/map_index_borrow_aliasing.il`、 `03_collections/map_value_kinds_churn.il`

**probe で「漏れない」ことを確認済みの周辺** (再調査不要): `m[new Key(1)]` index 読みの fresh key (escape 解析で stack 化)、 set の union/intersection/difference の fresh set 引数、 property getter の fresh receiver、 `s.add` の fresh 引数、 console.log の fresh string 引数、 文字列 literal 同士の concat。

### [解決済み記録] `Map<K, Object>` の overwrite で 24 bytes/iter leak は `__map_get` の読み出し retain と borrow 前提の消費側の二重 retain だった

**再現スクリプト** (修正済み、保管用):

```il
use std.test as test
class Box { n: i64; init(x: i64) { this.n = x } }
let m = new Map<string, Box>()
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    m["a"] = new Box(i + 1)         // 同じ key "a" に毎周上書き
    test.expect(m["a"].n, i + 1)
    i = i + 1
}
let after = test.liveAllocBytes()
console.log(`delta=${after - base}`)  // 修正前 4800 (= 200 × 24)、 修正後 24 (= map 内に生存中の最後の 1 個)
```

**真因 (2026-06-10 後続セッションで確定 + 修正、 `f3d0a899`)**:

当初仮説 (「`__map_set` の旧値 release 漏れ」) は外れ — overwrite release は [crates/ilang-runtime/src/maps.rs::__map_set](crates/ilang-runtime/src/maps.rs) に正しく入っていた (write-only ループでは leak しない)。 真因は読み出し側の規約矛盾:

- `__map_get` が読み出しごとに heap 値を retain して +1 で返していた (`d8c7f548` で導入)
- 一方 lowering の `is_fresh_object_expr` は `Index` の freshness を receiver に委譲するため、 `m[k]` (m が local) は「借用」と分類される
- その結果 `let x = m["a"]` は束縛時にもう 1 つ Retain を積み (stmt.rs の non-fresh 束縛規約)、 fn-body tail の `m[k]` も borrow retain を積む — ランタイムが付けた +1 を誰も release せず、 entry が上書き / delete された時点で +1 が永久に残る
- `arc_peephole::is_safe_to_cross` も「`MapGet` は自前で rc を増やさない」前提で whitelist しており、 ランタイムの retain は optimizer の前提とも矛盾していた

**修正**: `__map_get` の retain-on-read を撤去し、 Map の index 読みを `ArrayLoad` と同じ borrow 規約に統一。 `__map_get_optional` は fresh な Optional cell が +1 を所有するので retain を残す。 fresh receiver (`make_map()["k"]`) の経路だけ [crates/ilang-mir/src/lower/literals.rs::lower_index](crates/ilang-mir/src/lower/literals.rs) で「値 Retain → 孤児 map Release」 を発行して map release の value cascade による解放を防ぐ。

**検証結果**:

- 上記再現: delta=24 (合格条件 `delta < 1024` 達成)。 overwrite + alias + delete + clear + map drop を混ぜた churn では delta=0
- 別名健全性 (UAF なし): 「束縛 → 上書き / delete 後も旧値が読める」「fn tail で `m[k]` を返す」「`m.get(k)` Optional 経路」「string / array 値の overwrite + alias」「他コンテナへの移送」「fresh receiver index」 全 PASS
- `04_modules/events_basic.il` (retain-on-read 導入の動機だった EventEmitter) PASS
- workspace nextest: **525 / 525 PASS** (新 fixture 込み)
- nested_generic.il 16 並列 × 25 batch × 2 連続: **0/800**
- 回帰 fixture: `03_collections/map_overwrite_releases_prev_object.il` を追加

---

### [解決済み記録] `nested_generic.il` 系の SIGABRT は class method body の bare-var tail に retain が抜けていた

**真因 (2026-06-10 本セッションで確定 + 修正)**:

`class Box<T> { x: T; get(): T { x } }` のように **class method body の tail expression が bare var (= 暗黙の `this.field`)** のとき、 [crates/ilang-mir/src/lower/body_cx.rs:695](crates/ilang-mir/src/lower/body_cx.rs:695) の `tail_is_borrow` 判定が `ExprKind::Index { .. } | ExprKind::Field { .. }` の 2 形だけを borrow として扱い、 **`ExprKind::Var(name)` だが env / capture 解決できない (= class field) ケースを取りこぼしていた**。 その結果、 lower_block_hinted の scope-exit retain (L698-) が走らず、 戻り値が aliased のまま caller に返り、 caller は `lower_block` の scope-exit release で `release v7` を発行 → b1 の rc が 1 不足 → 後続の `release(b2)` の drop_fn による field release で b1 が freed → 直後の `release(b1)` (= let scope exit) が use-after-free。 malloc が同 slot の reuse 順序を ASLR で踏むかどうかで ~15% の確率 race として観測されていた。

`Box<i64>.get` (戻り値型 i64) は heap でないので発火条件を満たさず、 1 段 Box (= `Box<i64>` のみ) では race 0/400、 2 段 Box (= `Box<Box<i64>>` の field load が heap pointer) で初めて発火していた。

**修正**:

[crates/ilang-mir/src/lower/body_cx.rs::lower_block_hinted](crates/ilang-mir/src/lower/body_cx.rs:695) の `tail_is_borrow` に `ExprKind::Var(name)` arm を追加: `self.this_class.is_some() && env.lookup_binding(name).is_none() && capture にも居ない` ときは borrow とみなして scope-exit retain を発行する。 callee 側で正しい fresh +1 を caller に渡せて ARC が均衡する。

**検証結果**:

- nested_generic.il 最小再現 16 並列 × 25 batch × 2 連続: **0/800** (修正前 ~6% 発火、 dd40bc49 の `forget(compiled)` 撤去状態で再計測)
- workspace nextest: **525 / 525 PASS**
- 同時に dd40bc49 の `crates/ilang-cli/src/main.rs::run_file` の `std::mem::forget(compiled)` を撤去 (race 真因が消えたので drop も無罪)
- 周辺切り分けで得た事実 (本セッション保管用):
  - cranelift 0.131.1 → 0.132.1 への上げは race 確率不変で経路 A / B どちらにも効かず (= cranelift_module 自体は無罪)。 fix とは独立に依存上げは温存
  - Program / Function::value_tys / EnumLayout の MirTy は run_main 前後で完全一致 → ilang MIR data は intact、 corruption は ARC ref count 側
  - `process::exit(0)` で std rt cleanup を skip しても race が残った点が「真因は run_main 中」 を示唆していた

### [参考記録] 旧仮説と切り分け過程 (解決後の保管用)

`cranelift_module::ModuleDeclarations` の drop が真因方向 — という当初仮説の根拠と、 そこから真因へ辿った試行は次の通り。 同じパターンの race を将来踏んだときの足場として残す。

**最小再現** (約 15% で SIGABRT を 16 並列 × 25 batch で再現):

```il
use std.test as test
class Box<T> {
    x: T
    init(v: T) { this.x = v }
    get(): T { x }
}
let b1 = new Box<i64>(7)
let b2 = new Box<Box<i64>>(b1)
test.expect(b2.get().get(), 7)
```

**必須条件の絞り込み(本セッション)**:

各要素を 1 つずつ削った縮小版を 400 並列で測ったところ:

| variant | 必須要素 | race率 |
| --- | --- | --- |
| `Box 4 段だけ` | (test なし) | 0/400 |
| `Pair だけ` | (test なし) | 0/400 |
| `Box 2 段 + Box 構築のみ` | (`.get()` なし) | 0/400 |
| `Box 4 段 + Pair + test なし` | string あり、 test なし | 0/400 |
| `test.expect 3 回` | (Box なし) | 0/400 |
| `Box 2 段 + test.expect(b2.get().get(), 7)` | **全部** | **61/400 (15%)** |

→ **必須条件は「`use std.test as test` の取り込み + 2 段以上の `.get()` chain + その結果を test.expect の引数として渡す」**。 string は無関係、 Pair も無関係。

**stack trace (本セッションの crash_handler で取得 + `atos` 解決)**:

最初に検出した stack:

```
hashbrown::raw::RawTable::drop                                  ← SIGABRT 発火点
drop_in_place<cranelift_module::module::ModuleDeclarations>
drop_in_place<ilang_mir_codegen::compile::Compiled>
ilang::main at main.rs:75
```

これは `crates/ilang-cli/src/main.rs::run_file` で `std::mem::forget(compiled)` を追加して回避済み(子プロセスは数µs 後に exit するので drop しなくても OS が JIT メモリを回収する)。

ただし forget 後にも **別の drop path で同じ確率 (~6%) で再発**:

```
drop_in_place<ilang_mir::program::Function> + 384 (= 内部 Vec の drop)
drop_in_place<ilang_mir::program::Program>
ilang::main at main.rs:75
```

`Program::drop` 経路でも heap corruption 検出 → `__mir_alloc` の `__ILANG_HEAP_GUARD=1` ではこの path も 400/400 PASS なので、 こちらも `__mir_alloc` overrun ではない。 ASLR 依存で「特定 layout のときに Function 内の Vec が壊れる」path。 cranelift JIT generated code が ilang コンパイラ side の Vec を踏みうる layout 衝突を疑っているが未確定。

**ilang 側の影響**:

- ilang-mir / mir-codegen / lower すべてに `unsafe` 一切なし(grep で再確認済み)
- `ILANG_HEAP_GUARD=1` で 400/400 PASS → ilang runtime の `__mir_alloc` 経由ではない
- panic hook も呼ばれない → Rust panic 経由ではない

つまり cranelift_module / hashbrown のクライアント API の呼び方が壊している(declare_function / declare_data の重複登録、 ライフタイム違反、 ASLR-aware な内部状態を踏むサイズ依存)もしくは cranelift_module 自身の bug。

**cranelift バージョン上げは無関係と確定 (2026-06-10 本セッション)**:

`crates/ilang-mir-codegen/Cargo.toml` の cranelift 系 7 行を `0.131` → `0.132` に上げ (Cargo.lock 上は 0.132.1 で解決、 API breaking なし) て同じ最小再現を 16 並列 × 25 batch を 2 連続流したところ、 forget(compiled) を撤去した状態で 65/400 (16.25%) と 64/400 (16.00%) で発火、 0.131.1 時代の ~15% と統計差なし。 crash stack も `hashbrown::raw::RawTable::drop → drop_in_place<ModuleDeclarations> → drop_in_place<Compiled>` で 0.131.1 と完全に同じ経路。 → **cranelift_module 側の修正は 0.132.1 までに入っていない**ことが確認できたので、 依存上げは温存したまま forget(compiled) を戻して dd40bc49 状態を維持。

**次にやるべきこと**:

- 残る `Program::drop` race の真因究明: heap guard が捕まえない以上、 ilang コンパイラ side の Vec の中身が壊れている = `__mir_alloc` 由来でない別の overrun(JIT generated code が cranelift JIT page 経由で書き出すアドレス計算の bug が一番濃い)。 一段切り分けには次の手:
  - `Program` 自身も `std::mem::forget` してみて race が消えるかで「真因が drop の中か別か」を判別(本セッションでは Plan を守って forget(compiled) のみ commit)
  - 残る方なら `process::exit(0)` で全 drop を skip し、 fixture suite の flake を完全に消す(JIT child は短命なので無害)
- 「2 段 .get() chain → test.expect」 path で mono 化された method の declare_function を全部 dump して、 重複 declare がないかを確認:
  - `Box_i64.get`, `Box_Box_i64.get` のような mangled name が unique か
  - declaration 順序、 linkage、 signature の一貫性
- 次に試すなら cranelift 0.133 以降 (本セッションでは最新が 0.132.1 だったが無効)。 ただし 0.131 → 0.132 で何も改善しなかった事実から、 期待値は低い
- もしくは hashbrown のバージョン固定で minimal reproducer を作って cranelift / hashbrown 側に issue report

**確認手順(最小再現)**:

```sh
cat > /tmp/nest_v6.il <<'EOF'
use std.test as test
class Box<T> {
    x: T
    init(v: T) { this.x = v }
    get(): T { x }
}
let b1 = new Box<i64>(7)
let b2 = new Box<Box<i64>>(b1)
test.expect(b2.get().get(), 7)
EOF
ILBIN=./target/release/ilang
mkdir -p /tmp/nest_v6_check
for batch in $(seq 1 25); do
  for j in $(seq 1 16); do
    idx=$(( (batch-1)*16 + j ))
    ( ILANG_TRACE_CRASH=1 $ILBIN run /tmp/nest_v6.il > /tmp/nest_v6_check/${idx}.out 2>&1; echo $? > /tmp/nest_v6_check/${idx}.code ) &
  done
  wait
done
cat /tmp/nest_v6_check/*.code | sort -n | uniq -c
# 期待: ~85/400 PASS、 ~15% で 134 と stack trace 付き
```

### [解決済み記録] `crepr_struct_assign_index_field.il` 系の確率的失敗

**現状の観測**(本セッションで `programs.rs::check()` に signal 出力を追加して観測力強化済み):

- `cargo nextest run -p ilang --test programs run_all_program_fixtures` の 20 連続実行で:
  - `crepr_struct_assign_index_field.il`: signal=11 (SIGSEGV) × 1
  - `nested_generic.il`: signal=6 (SIGABRT) × 2
- bash で `./target/release/ilang run <fixture> & × 16` × 13 batch (= 200 並列起動) でも `crepr_struct_assign_index_field.il` は 200 中 3〜15 件 (1.5%〜5%) で exit 134 (= SIGABRT)。 つまり **nextest harness 限定ではなく、 多数並列起動で素直に踏む**。

**stderr 空の謎は本セッションで部分解明**:

`crates/ilang-runtime/src/enums.rs:122-134` の「CRepr struct 経由で読んだ enum discriminant が不正」path は `eprintln! → process::abort()` だが、 `eprintln!` が pipe buffer に届く前に `abort()` が走り、 stderr が空のままプロセスが死ぬ。 本セッションで該当 4 箇所(enums.rs ×2 + regex.rs ×2)に `stderr().flush()` を入れたが、 並列再現時の stderr 空は **直らない** — つまり「abort 前 flush」では救えない別の経路で死んでいる(本物の SIGSEGV / heap corruption が `eprintln!` 到達前に発火)。

**最小再現と絞り込み**:

- 元 fixture を縮めて再現: `let s0 = new Slot(); let s1 = new Slot(); let arr: Slot[] = [s0, s1]; arr[1].kind = Mode.active; arr[1].seq = 99` (16 並列 × 25 batch = 400 試行で 6/400 SIGABRT/SIGSEGV)。
- **`arr[0]` への書き込みは安全、 `arr[1]` (idx ≥ 1) への書き込みのみ race**。
- MIR dump: `Slot[]` は inline CRepr struct array (stride 8 byte) として lower される。
- `__mir_alloc` を `Vec<u8>` → `Vec<u64>` に変えて alignment を厳密化しても **race は同確率で続行**(alignment 仮説は外れ)。

**lldb で stack trace を取得 (race の真因方向確定)**:

debug build で並列負荷下に lldb attach loop を回し、 2 系統の crash trace を捕まえた:

**系統 A**: `EnumLayout::repr: MirTy` 経由

```
frame #0:  drop_in_place<MirTy>((null)=0x8)
frame #5:  alloc::alloc::dealloc(ptr=stack address)
frame #10: Box<MirTy>::drop
frame #12: drop_in_place<MirTy>
frame #13: drop_in_place<EnumLayout>
frame #14: drop_in_place<[EnumLayout]>
frame #15: Vec<EnumLayout>::drop
frame #17: drop_in_place<Program>
```

**系統 B**: `Function::value_tys: Vec<MirTy>` 経由 (4 回連続観察)

```
frame #0: drop_in_place<MirTy>((null)=0x0000000000000008)
frame #1: drop_in_place<Box<MirTy>>((null)=0x15370f1a0)
frame #2: drop_in_place<MirTy>((null)=0x15370f190)        ← 最深 nest
frame #3: drop_in_place<Box<MirTy>>((null)=0x1540096a8)
frame #4: drop_in_place<MirTy>((null)=0x154009698)        ← 中間 nest
frame #5: drop_in_place<[MirTy]>
frame #6: Vec<MirTy>::drop                                ← value_tys
frame #8: drop_in_place<Function>
frame #9: drop_in_place<[Function]>
frame #10: Vec<Function>::drop
frame #12: drop_in_place<Program>
frame #13: ilang::run_file at main.rs:1082
```

malloc error: `pointer being freed was not allocated`, address `0x16fdf6700` (stack address) または `0x8` (null+offset)。 **共通パターン: Box<MirTy> の内部 pointer 値が 0x8**(= Vec の len field 等として読まれるサイズ)。

**race は ilang コンパイラ自身の memory corruption**:

- fixture 実行コード (= ilang JIT generated) ではなく、 ilang コンパイラの `Program::drop` 時に `EnumLayout::repr` または `Function::value_tys` の中の MirTy のネストした Box が **dangling pointer (stack address or null+offset)** を保持して dealloc で abort。
- 共通パターン: 2 段 `Box<MirTy>` nest の最深部 (`Array<X>` / `Optional<X>` / `Promise<X>` / `RawPtr<X>` / `Set<X>` / `Map<K,V>` / `FnTy::ret` のうち X 自身も `Box<MirTy>` を持つ variant)で内部 pointer 値が **`0x8`** に化けている。
- 最小化 fixture (`crepr_struct_assign_index_field.il`) の post-lower MIR には 2 段 nest が明示的に登場しない → mono / opt pass / codegen のどこかで動的に生成された MirTy が corrupt している強い疑い。
- 触る場所候補 (value_tys に push する箇所、 grep 済み):
  - [crates/ilang-mir/src/builder.rs:68](crates/ilang-mir/src/builder.rs:68) — lower 中の builder
  - [crates/ilang-mir/src/passes/inline.rs:270](crates/ilang-mir/src/passes/inline.rs:270) — inline pass の `caller.value_tys.push(ty)`
  - [crates/ilang-mir/src/lower/decl/extern_c.rs:442,539](crates/ilang-mir/src/lower/decl/extern_c.rs:442) — extern C struct synth
  - [crates/ilang-mir/src/lower/decl/enum_fn.rs:203](crates/ilang-mir/src/lower/decl/enum_fn.rs:203) — enum fn synth
- 他に `dce_fn.rs` の `mem::take(&mut prog.functions)` で再配置する path もある。
- ilang-mir / parser / mir-codegen に `unsafe` 一切なし(grep で確認済み)ので、 直接的な unsafe deref ではない → **論理的 buffer overrun** か **意図しない alias** が source。
- ASLR でメモリレイアウトが変わるので踏む / 踏まないが分かれる典型 UB pattern。

**ASAN は timing 変化で踏まず** (debug + release 両方確認):

`RUSTFLAGS="-Z sanitizer=address" cargo +nightly build -Z build-std --release --target aarch64-apple-darwin -p ilang` で ASAN release ビルドを試したが、 400 並列起動で 400/400 PASS。 ASAN は debug でも release でも race を踏ませない。

**真因 pass 切り分け結果(opt pass は無関係)**:

[main.rs:484-501](crates/ilang-cli/src/main.rs:484) の 6 つの opt pass 各々を env (`ILANG_NO_DCE_FN` / `ILANG_NO_PROMOTE_LOCALS` / `ILANG_NO_INLINE` / `ILANG_NO_CONST_FOLD` / `ILANG_NO_BRANCH_FOLD` / `ILANG_NO_DCE`) で個別 disable して 400 並列を測ったところ、 race 件数 (27〜39 / 400) は baseline (33 / 400) と統計的に有意差なし。 **6 つの opt pass はどれも真因ではない**。 残る容疑者:

- **MIR lower** (`crates/ilang-mir/src/lower/`、 `builder.rs::value_tys.push`)
- **mono pass** (`crates/ilang-mir/src/monomorphize/`、 env では disable できない)
- **mir-codegen** (cranelift IR 生成中の MirTy 走査)
- **ilang-runtime** の何か (heap-trace の OnceLock 等は可能性低い)

ただし `crepr_struct_assign_index_field.il` は generic を使わないので mono pass は事実上 no-op になるはず → **lower か codegen** が最有力。

**[真因確定 + 修正完了]** [crates/ilang-mir-codegen/src/compile/lower_inst/objects.rs:755](crates/ilang-mir-codegen/src/compile/lower_inst/objects.rs:755) の `StoreField` codegen が **field の宣言型 (`field_meta_ty`) ではなく value の型 (`val_ty_mir`) で store size を選んでいた** のが真因。 例えば `seq: i32 = 99` の lower:

- 右辺 `99` の MirTy は `I64` (const のデフォルト)
- 旧コード: `celem_clif_type_with_enum(prog, &val_ty_mir)` で I64 → `Some(I64)` の guard が `!= I64` 不一致で `_` arm に流れ、 **i64 store (8 byte)** を発行
- 結果: `Slot.seq` (offset 4..8) を書くつもりが、 store 命令は `c_off+8 = offset 4..12` を書く → CRepr struct buffer (8 byte) を 4 byte 越境
- `Slot[]` data buffer (= 2 × 8 byte) の場合、 `arr[1].seq = 99` で `(data_ptr + 8) + 4 = data_ptr + 12` に 8 byte 書き込み = **offset 12..20** = **buffer の offset 16..20 (4 byte) を tail に踏み出す**

heap guard で 100% 確認した「offset 16 への single byte zero write」 は、 実は **「offset 12..20 への 8 byte i64 store」の高位 4 byte (うち最下位 1 byte が `0xBE → 0x00` に書き換わって見えた)**だった。 (`99 = 0x00000000_00000063` の little-endian で高位 4 byte は全 0 なので、 guard の最下位 byte 0xBE が `0x00` に書かれて見える。)

**修正**: `field_meta_ty.as_ref().unwrap_or(&val_ty_mir)` で field 宣言型を優先して store 幅を選ぶ。 i32 field なら ireduce で 4 byte に truncate してから store。

**検証結果**:

- `ILANG_HEAP_GUARD=1` で 400 並列起動: **0/400 abort** (修正前 400/400)
- guard なしで 400 並列起動: **0/400 race** (修正前 ~33/400)
- nextest `run_all_program_fixtures` 10 連続: **10/10 PASS**
- workspace 全体: **525/525 PASS**

---

### [研究記録] heap guard 計装で真因 path を pin point

[crates/ilang-runtime/src/alloc.rs](crates/ilang-runtime/src/alloc.rs:34) に `ILANG_HEAP_GUARD=1` で各 alloc の前後に 16 byte ずつ `0xDEADBEEFCAFEBABE` の guard を埋める計装を追加。 free 時に guard を検査して破壊を検出。

並列 400 試行で **400 / 400 が exit 134** (= SIGABRT)、 全 run で **同一の corruption pattern**:

- 場所: **`size=16` の buffer の tail guard (offset 16)**
- 破壊値: `0xDEADBEEFCAFEBA00`
- 元値: `0xDEADBEEFCAFEBABE`
- 差分: **最下位 1 byte だけ `0xBE → 0x00`** = **off-by-one single byte zero write**

`ILANG_HEAP_TRACE=1 ILANG_HEAP_GUARD=1` の組合せで死亡直前の alloc 順序を取得:

```
[alloc] size=8   ptr=0x154009880   ← s0
[alloc] size=32  ptr=0x154009a70   ← Mode.idle cached enum box
[alloc] size=8   ptr=0x15400c1a0   ← s1
[alloc] size=48  ptr=0x15400c1d0   ← array header
[alloc] size=16  ptr=0x15400c220   ← array data
[alloc] size=32  ptr=0x15400c250   ← Mode.active cached enum box
[guard] CORRUPTION at ptr=0x15400c220 size=16: [("tail", 0, ...)]
```

`Mode.active` enum box alloc 後、 array data (size=16) を free しようとした時に guard check が発火。 つまり **array data alloc → Mode.active alloc → arr[1].kind = active 系の JIT generated 操作 → array data free** の流れで、 何かが 1 byte 0 を array data の offset 16 に write している。

`size=16` は `Slot[] = [s0, s1]` の 2 要素 × 8 byte stride の data buffer。 corruption が **offset 16 ジャスト**(= データ末尾の直後) ということは、 cranelift JIT が出した ARM64 命令の中に「`strb wzr, [data_ptr, #16]`」相当の **off-by-one byte store** がある。

最小化 fixture で 1 byte zero store を出しうる codegen path:

- `crepr_struct_copy` の最後の 1-byte ループ([lower_inst/mod.rs:58-62](crates/ilang-mir-codegen/src/compile/lower_inst/mod.rs:58)) — total=8 では到達しないはず
- StoreField の CReprEnum (kind: u16) は `store(I16, ...)` で 2 byte
- NewArray の `crepr_struct_copy(fb, raw, dst_addr, total=8)` を `i=0, 1` で 2 回
- ArrayLoad は読み出しのみ

cranelift IR dump (`fb.func.display()` 相当)を取って、 array data buffer 周辺の store 命令の offset を全部確認するのが次の手。

**全箇所レビュー結果(同ラウンド前半で実施)**:

**A. `Function::value_tys: Vec<MirTy>` を mutate する production code は 5 箇所のみ**:

| 場所 | 操作 | 入力 ty の出所 |
| --- | --- | --- |
| [builder.rs:68](crates/ilang-mir/src/builder.rs:68) | `value_tys.push(ty)` | lower 全箇所から(BodyCx::new_value) |
| [lower/decl/enum_fn.rs:203](crates/ilang-mir/src/lower/decl/enum_fn.rs:203) | `value_tys.push(params[i].clone())` | enum fn synth の param type |
| [lower/decl/extern_c.rs:442](crates/ilang-mir/src/lower/decl/extern_c.rs:442), [539](crates/ilang-mir/src/lower/decl/extern_c.rs:539) | `value_tys.push(pty.clone())` | extern C struct method synth |
| [passes/inline.rs:270](crates/ilang-mir/src/passes/inline.rs:270) | `caller.value_tys.push(ty)` | `cand.value_tys.get(v.0).cloned()` の結果 |
| [passes/dce_fn.rs:223](crates/ilang-mir/src/passes/dce_fn.rs:223) | `for ty in &mut f.value_tys { remap_ty(ty, ...) }` | in-place mutate (`Object(cid)` の cid 書き換えのみ) |

opt pass の elimination matrix から 4, 5 はすでに除外。 残るは **1, 2, 3 (lower 系) で push される ty の出所**。

**B. `Box<MirTy>` を持つ MirTy variant の構築箇所**:

- 静的 (`Box::new(MirTy::primitive)`): 22 箇所、 全部 1 段 nest、 safe。
- **動的** (`Box::new(self.resolve_ty(...))`): [lower_state.rs:210-281](crates/ilang-mir/src/lower/lower_state.rs:210), [body_cx.rs:973-1028](crates/ilang-mir/src/lower/body_cx.rs:973) で再帰呼び出し。 ilang ソースの型注釈の深さに応じて任意段の nest を構築 (例: `Array<Optional<Box<T>>>`)。

**C. 2 段 `Box<MirTy>` nest を生成しうる入口**:

- ソース内の `let x: Array<Optional<T>>` のような明示的多段型注釈
- 暗黙的に Optional を包む path (例: `Array<T?>`、 stdlib `Promise<T>` chain、 Map の key/val)
- 最小化 fixture (`crepr_struct_assign_index_field.il`) では明示的にはない → **`use std.test as test` 経由の stdlib lower で動的に 2 段 nest が作られる可能性**

**D. ilang-mir / mir-codegen / lower すべてに `unsafe` 一切なし** (改めて確認)。 つまり Rust safe code 範囲で起きる UB:

- 依存 crate (cranelift / hashbrown 等) の `unsafe` が source の可能性
- Vec の grow / shrink が valid な範囲を超える(`usize::MAX`)系の panic は別経路で死ぬ
- `mem::take` → `extend` → 各種 mutation で **意図しない alias** ができている path(grep 限界、 動的解析必要)

**E. opt pass の elimination matrix(本ラウンドで取得)**:

| 環境変数 | 異常終了率 |
| --- | --- |
| baseline (no env) | 33 / 400 (8.25%) |
| ILANG_NO_DCE_FN=1 | 36 / 400 |
| ILANG_NO_PROMOTE_LOCALS=1 | 31 / 400 |
| ILANG_NO_INLINE=1 | 35 / 400 + 1 SIGSEGV |
| ILANG_NO_CONST_FOLD=1 | 32 / 400 |
| ILANG_NO_BRANCH_FOLD=1 | 27 / 400 |
| ILANG_NO_DCE=1 | 39 / 400 |

統計的有意差なし。 opt pass はすべて真因から除外。

**次にやるべきこと**:

- **動的解析が必須**: 静的レビューでは pin point 不可。
  - **Miri** (`cargo +nightly miri run -p ilang -- run fixture.il`) — 但し cranelift JIT を含むため動作するかは未確認。
  - **AddressSanitizer の wall-clock 緩和**: ASAN debug / release ともに timing が変わり race 引かない既存実績。 `RUSTFLAGS=-Zsanitizer=address -C opt-level=3 -C target-cpu=native` 等のチューニングで縮められる可能性。
  - **printf debug**: `Vec<MirTy>` の grow タイミングごとに `len/cap/ptr` を stderr に吐く instrumentation を入れて、 並列起動で踏んだ run の Vec 状態を比較。
- ASAN なしの最終手段: ilang コンパイラ内の Vec / Box 全部を `with_capacity` で十分大きく確保 → grow を抑制 → 並列で再現するか比較(真因仮説「grow 中の何か」の検証)。

**真因追跡で試して効かなかった手段** (次セッションで違う方法を考えること):

- macOS の core dump (`ulimit -c unlimited` + `/cores/core.%P`): `sysctl kern.coredump=1` でも `/cores` に書き出されず。 macOS の code signing / `get-task-allow` entitlement 制約。
- lldb で attach (`lldb -- ilang run ...`): デバッガ attach すると timing が変わり、 race を引かなくなる。
- `RUST_BACKTRACE=1`: stderr 空のまま死ぬので backtrace も出ない。

**次にやるべきこと**:

- **ASAN(AddressSanitizer)で計装**: nightly Rust + `-Z sanitizer=address` で `ilang` を再ビルドして並列起動 → double free / use-after-free をピン点。 仮説では `Slot[] = [s0, s1]` の elem 値コピー後の release path で double-free が起きている。
- `ilang` バイナリに `get-task-allow` entitlement を付ける(`codesign --entitlements ent.plist -s - target/release/ilang`)→ core dump を `/cores` に書き出させる → 死亡時の core を lldb で読む。
- `MallocStackLogging=1` + `MallocScribble=1` 等の macOS malloc debug 環境変数で heap corruption を捕まえる(性能落ちて timing 変わるリスクあり)。
- 最小化済みの `let s0 = new Slot(); let s1 = new Slot(); let arr: Slot[] = [s0, s1]; arr[1].kind = X; arr[1].seq = Y` で集中的に再現してから上記。

**確認手順**:

```sh
# 並列起動 (200 個) で確率失敗を引く
FIXTURE=crates/ilang-cli/tests/programs/05_edge_cases/crepr_struct_assign_index_field.il
ILBIN=./target/release/ilang
mkdir -p /tmp/raceCheck; rm -f /tmp/raceCheck/*
for batch in $(seq 1 13); do
  for j in $(seq 1 16); do
    idx=$(( (batch-1)*16 + j ))
    ( $ILBIN run $FIXTURE > /tmp/raceCheck/${idx}.out 2>&1; echo $? > /tmp/raceCheck/${idx}.code ) &
  done
  wait
done
cat /tmp/raceCheck/*.code | sort -n | uniq -c
# 期待: 1〜5% の確率で 134 (SIGABRT) が出る
```

### 関連 commit 履歴 (時系列)

- `f2eea6e3` AssignField で CRepr-parent Enum field を retain/release から除外 (point fix、 papering over)
- `c97cc0b2` ↑の pin fixture (`crepr_struct_enum_field_assign.il`)
- `28f7060f` → `65bb326a` → `14292c5e` `MirTy::CReprEnum` 導入で papering over を解消
- `84f2eb6c` CReprEnum 関連 fixture 6 件 (全 PASS)
- `a6e9310e` 疑わしい path を網羅する fixture 9 件 (1 件 fail = `crepr_struct_field_discard.il`)
- `d4b44d2f` 修正試行 3 件 (実質効果なし、 撤回せずに残置)
- `bcd3367f` `ILANG_HEAP_TRACE` env と真因確定の trace 結果記録
- `4d1f97dc` 内部 fn CRepr struct return の sret 経路化 (`Inst::Call` 系)
- `3bb34848` `Inst::VirtCall` の sret 経路を内部 fn 規約に統一
- `c8a8f525` `Inst::CallIndirect` (closure 経由) の sret + by-value param 経路追加
- `56d5881e` CRepr struct return まわりの回帰防止 fixture 4 件
- **本セッション** (2026-06-10) `test.liveAllocCount` / `test.liveAllocBytes` で測定前に `pool::drain` を呼ぶことで async 系 leak 検知の確率性を解消。

## 実装済み機能 (一覧)

### コア
- 全 10 数値型 (`i8/i16/i32/i64/u8/u16/u32/u64/f32/f64`) + bool + string + Unit
- 整数リテラル: 10進 / 16進 (`0xff`) / 8進 (`0o755`) / 2進 (`0b1011`) + `_` 桁区切り
- 数値型サフィックス (`1_i32`, `1.5_f32`)
- 暗黙の型変換規則 (同符号整数間 / 整数→浮動 / 浮動↔浮動)、符号またぎと浮動→整数は `as` 必須
- **二項演算でのリテラル側型適応**: `u32_var != 0` のように相手の整数型にリテラルが収まれば自動でその型として扱う

### 制御フロー
- `if` / `elif` / `else` (式)
- `while` / `loop` / `for in`
- range 式 `a..b` (排他) / `a..=b` (包含) — for-in イテレータ位置のみ
- `break` / `continue` / `break v` (loop からの値付き脱出)
- `return` (値あり / なし) — **トップレベルでも使える** (早期 program exit、値は持てない)
- `match` (enum 上のパターンマッチ)
- `if let some(v) = x` (Optional 専用パターン)

### 関数
- `fn` 宣言 (引数型必須、戻り値型 `: T` (TS 風))
- ジェネリック関数 `fn id<T>(x: T): T { x }` — 推論ベースで JIT mono 化。`*const T` のような raw pointer 内の TypeVar も推論される
- 関数オーバーロード — best-match scoring、ambiguous エラー
- ファーストクラス関数 (`let f = add; f(2, 3)`)
- 匿名関数 `let inc = fn(x: i64): i64 { x + 1 }`
- クロージャ (読むだけ=値スナップショット / 代入=共有、全 capture 型対応)
- `@requires(...)` 等の属性 (パースのみ、enforce は未実装)
- `@override` (継承メソッドの override マーカー、必須)

### FFI (`@extern(C) { ... }`)
ブロック構文に統一済み。`@lib(...)` の dlopen は JIT 起動時に解決される。

ブロック内で書ける item:
- `fn name(...): T` — 関数宣言
  - `@lib("libfoo", "libfoo-fallback.so")` — dlopen するライブラリ名(複数指定 = フォールバック)。省略すると host 登録形(stdlib の math/os/test がこの形)
  - `@optional` — ライブラリやシンボルが見つからなくても JIT 構築は失敗せず、呼ぶとアボートするスタブにバインド。`os.libLoaded(name)` で事前ガードする
  - `@symbol("c_name")` — C 側のシンボル名と ilang 側の fn 名を分離
  - 末尾の `...` — printf 系 variadic
- `fn name(...): T { body }` — ilang 本体を C ABI で公開する関数(callback / 内部 wrapper)
- `struct Name { ... }` — C 互換構造体。空 struct = opaque handle として使う
- `union Name { ... }` — C union (全フィールド offset 0)
- `static name: T` — C グローバル変数
- `class Name { ... }` — ARC-managed wrapper クラス。method 本体は in_extern_c 文脈で動くので、生 extern fn と FFI ヘルパーを直接呼べる(deinit で C ハンドル自動 close できる)
- `@packed` (struct のみ) / `@bits(N)` (フィールド) — C のレイアウト調整に対応

ブロック**内のみ**で使える型:
- `*T` / `*const T` (raw pointer)
- `char` / `void` / `size_t` / `ssize_t`

これらの型はブロック外の式・型注釈に書けない。値も漏らせない (let バインディングで受けたり、call 結果に C-only 型が含まれると型エラー)。

ブロック内のみで呼べる **マーシャリングヘルパー**(自動的にビルトイン登録):
- `cstrFromString(s: string): *const char`
- `stringFromCstr(p: *const char): string`
- `freeCstr(p: *const char)`
- `bytesFromBuffer(p: *const void, n: size_t): u8[]`
- `arrayFromCArray<T>(p: *const T, n: size_t): T[]` (T は数値プリミティブ / bool)
- `cstrArrayToStrings(p: *const *const char): string[]`
- `errnoCheck(rc: i32): i32?` / `errnoCheckI64(rc: i64): i64?`

その他のキャスト規則 (ブロック内):
- `*T ↔ *U` — type-pun (`*const u8 → *const void` 等)
- `*T ↔ i64` — pointer ↔ アドレス値
- `T[] → *T` (Array→RawPtr 暗黙変換、data ポインタを渡す)
- struct 値渡し(< 16 B = chunks / HFA / > 16 B = sret)を自動で適用(旧 `byValue` フラグ相当)

### モジュール / プロジェクトファイル
- `use module` (whole) / `use module { foo, bar }` (selective: bare 名 + 名前空間の両方が使える)
- **`use module as foo`** — 別名で名前空間を import (`foo.X` で参照、内部的には `module.X` に書き戻される)
- **`use module as _ { ... }`** — 名前空間を抑止し、selective 名のみ公開
- **`pub use module`** — re-export(umbrella module を作る用)。`as` の併用は不可
- **可視性**: top-level item とクラスメンバはデフォルトで module-private。`pub fn` / `pub class` / `pub enum` / `pub const` / `pub let` (top-level) / `pub` 付きの `@extern(C){}` 内アイテム、`pub init` / `pub <method>` / `pub <field>` / `pub static` / `pub get/set` で外部公開。loader は post-load の `validate_visibility` で selective import と `module.X` 参照を pub catalog に照合し、`pub use M` チェインを辿って可視性を伝播する。`pub use M` は M の **pub アイテムだけ** を再エクスポート
- **`ilang.toml`** プロジェクトファイル: `[deps] sdl2 = "path"` で `use` の探索パスを追加。CLI が entry file から上に辿って自動発見
- `const NAME: T = const_expr` — 算術 / ビット / 比較 / 論理 / `as` キャスト / 他の const 参照を**コンパイル時に折りたたみ**。型注釈付き const は substitute 時に Cast で wrap されて、参照箇所すべてに自動的に型が伝わる
- 同梱モジュール: `math` (sqrt/sin/cos/pi/e ほか) / `test` (expect/...)、`os` (errno / libLoaded / 定数群)

### クラス (OOP フル)
- `class C { fields; init(); methods; deinit() }` — `init` 可、`deinit` 可
- 暗黙 `this` (フィールド/メソッドを `this.` なしで参照可)
- `==` / `!=` は参照等値 (`Rc::ptr_eq`)
- ジェネリッククラス `class Box<T> { ... }` — JIT mono 化
- メソッド/init オーバーロード (best-match scoring)
- `get` / `set` プロパティ (`obj.x` がアクセサ呼び出し)
- `static` メソッド (`ClassName.method(args)`)
- `static` フィールド (`i64`/`f64`/`bool` のみ、定数式初期化、mutable)
- 継承 (`extends`): 単一継承、`@override` キーワード必須、`super.method(...)` / `super(...)` (init 連鎖)、仮想ディスパッチ (vtable)、サブタイプ

### コレクション
- 配列 `T[]` / `T[N]`: literal / index / push / pop / length / slice / indexOf / includes / map / filter / forEach
- Map `Map<K, V>` (K = string / int / bool): `m[k]` / `m[k] = v` / has / delete / size / keys / values / get
- Optional `T?`: `none` / `some(x)` / `if let` / `is_some` / `is_none` / `unwrap` / `T → T?` 自動 wrap
- Weak `T.weak`: `.get(): T?` / `Foo → Foo.weak` 自動 downgrade / 二重 rc
- enum + 構造体的 payload (`tuple` / `struct` / `unit` バリアント)
- Result<T, E>: 組み込みジェネリック enum、`Result.ok(v)` / `Result.err(e)` で構築

### 文字列
- リテラル + エスケープ (`\n` `\t` `\r` `\\` `\"` `\0`)
- `+` (連結)、`==` `!=` (構造的等値)
- メソッド: `length` (Unicode コードポイント) / `charAt` / `includes` / `startsWith` / `endsWith` / `toUpper` / `toLower` / `trim` / `split` / `replace` / `slice`
- 文字列補間 (テンプレートリテラル) 対応: `` `val=${x} sum=${x + 1}` ``

### メモリ管理 (ARC)
- 全ヒープ値は ref-counted: Object / String / Array / Optional / Weak / Map / closure / EnumHeap
- `deinit` がスコープ脱出時 / rc=0 時に発火
- 二重 rc (strong/weak) で循環参照を `T.weak` で解消可能
- フィールド / 配列要素 / capture の再帰 release

## 実行モデル

| モード | コマンド | 用途 |
| --- | --- | --- |
| **MIR JIT** | `ilang run path.il` | Cranelift ネイティブコード、唯一の実行経路 |
| **AOT** | `ilang build path.il -o out` | 同じ MIR→Cranelift 経路を ELF/Mach-O に焼き出す |
| **REPL** | `ilang` (引数なし) | 1 行ずつ評価 (MIR JIT を REPL スロット付きで実行) |

`ilang.toml` が entry の上の階層にあれば自動発見、`[deps]` のパスが `use` の探索先に追加される。`ilang run --mir-jit` は旧 CLI の互換フラグで現在はデフォルトと同じ。

現状の制約:
- 静的フィールドは `i64` / `f64` / `bool` のみ (string / object 等は未対応)
- ジェネリッククラスでの **継承** / **静的メンバー** / **プロパティ** は型パラメータ解決の制約により未対応

## ワークスペース構成

```
crates/
├── ilang-ast/       # AST 定義 (Span 含む)
├── ilang-lexer/     # 字句解析 (Token, leading_newline, numeric_suffix)
├── ilang-parser/    # Pratt 構文解析 + loader (use 解決 / pub use / ilang.toml dep paths) + normalize + const 折りたたみ
├── ilang-types/     # 型チェッカー (overload resolution / mangle / inheritance / closures / @extern(C) コンテキスト)
├── ilang-mir/       # AST→MIR (SSA + block-args)、モノモーフィゼーション、validator/printer
├── ilang-mir-codegen/ # MIR→Cranelift JIT 本体
│   ├── compile/           # ARC + FFI + REPL slot を含む lowering 一式
│   ├── aot/               # ELF/Mach-O を吐く `ilang build` 経路
│   └── ty.rs              # 内部 JIT 型 / クラスレイアウト
├── ilang-runtime/   # ランタイム (alloc, retain/release, str/array fns、math/os/test extern, native_extern)
├── ilang-lsp/       # LSP サーバー
└── ilang-cli/       # `ilang` バイナリ (REPL + run + build + ilang.toml 解決)

bindings/
├── cocoa/           # macOS Cocoa バインディング (foundation / appkit)
├── directx12/       # Windows DirectX 12 バインディング (テストフィクスチャ付き)
├── gtk4/            # Linux GTK 4 バインディング (テストフィクスチャ付き)
├── libc/            # POSIX libc バインディング
├── sdl2/            # 再利用可能な SDL2 バインディング (umbrella + 機能別 6 ファイル + README)
├── sqlite3/         # SQLite3 バインディング
└── windows/         # Windows Win32 バインディング

examples/
├── sdl_breakout/   # SDL2 を使ったゲーム画面サンプル (main.il + ilang.toml)
└── libs/gui/       # libs/gui のサンプル群 (controls / menus / window 等)

libs/
└── gui/             # クロスプラットフォーム GUI ライブラリ (cocoa/win32/linux backend)

docs/syntax.md       # ユーザー向け構文一覧 (常に最新に保つ)
crates/ilang-cli/tests/programs/  # 150 個の .il fixture (MIR JIT + AOT で実行、stdout parity 検証)
```

各 crate は `lib.rs` がほぼ re-export だけ。実体は役割別ファイル。**新コードを書くときも役割別モジュールを維持** すること。テストは `crates/<crate>/tests/<name>.rs` の統合テスト + `crates/ilang-cli/tests/programs/*.il` の言語レベル fixture。

### .il fixture の書き方
- `crates/ilang-cli/tests/programs/<カテゴリ>/<名前>.il` に `.il` ファイルを 1 つ置けば自動で MIR JIT + AOT ビルドで実行される
- マジックコメント:
  - `// expect: <line>` — stdout の行を順序通りマッチ
  - `// expect-error: <substr>` — 失敗を期待、stderr に substr が含まれること
  - `// jit: skip` — MIR JIT 実行をスキップ
  - `// aot: skip` — AOT ビルド経路をスキップ
- MIR JIT と AOT 両方が走った場合は stdout 一致も検証 (divergence 防止)
- アサーションは `use test; test.expect(actual, expected)` などで書く

## JIT メモリレイアウト (重要)

| 値 | レイアウト | ヘッダサイズ |
| --- | --- | --- |
| Object | `[strong_rc | weak_rc | drop_fn | vtable_ptr | fields...]` | 32 byte |
| String | `Box<StringRc { rc, s }>` (リテラルは saturated rc) | — |
| Array | `[rc | drop_fn | len | cap | data_ptr]` ヘッダ + 別領域データバッファ | 40 byte |
| Optional<heap> | T と同じ i64 ポインタ (0 = none) | — |
| Optional<primitive> | ヒープ box (rc + value) | 16 byte |
| Weak<T> | T と同じ i64 ポインタ。retain/release は weak 側 helper | — |
| Map | Box<HashMap<MapKey, i64>> | — |
| EnumHeap | `[rc | weak_rc | drop_fn | vtable | tag(i32) | padding | payload...]` | object と同じ |
| Closure | `[rc | drop_fn | total_size]` ヘッダ + `[fn_ptr | env_field0 | ...]` | 24 byte |
| Function value | closure ptr (top-level fn は trampoline closure に自動 wrap) | — |
| `@extern(C)` struct | ARC ヘッダ付きヒープ Object と同じ — C には負オフセットの ARC ヘッダは見えず、ユーザポインタ = フィールド領域先頭 | 32 byte |

### vtable (継承)
- per-class `Box<[i64]>` (typechecker が assign した slot に関数ポインタ)
- 子クラスの vtable は親の prefix を含む (slot 共有)
- `obj.method()` は `vtable_ptr → vtable[slot] → call_indirect`
- `super.method()` は親の特定関数への直接呼び出し (no indirection)

## 重要な設計決定 (引き継ぎ要)

| 領域 | 決定 | 理由 |
| --- | --- | --- |
| 所有権 / `mut` | 採用しない | ARC 前提、全変数再代入可 |
| 戻り値型構文 | `): T { ... }` | TS 風、`->` トークンなし |
| 例外 | 採用しない | Result + panic、Rust/Go/Zig 流 |
| ファイル拡張子 | `.il` | 確定 |
| capability | アノテーション + capability 値 (案C) | enforce はまだ |
| ネイティブコード | Cranelift 第一候補 | 軽量、ビルドコスト優先 |
| クラス継承 | 単一継承 + 仮想ディスパッチ | 採用済み |
| コンストラクタ名 | `init` (Swift 風) | 特殊メソッド名、キーワードではない |
| オブジェクト等価性 | 参照等価 (`Rc::ptr_eq`) | structural equality は将来トレイト経由 |
| クロージャキャプチャ | 読むだけ=値スナップショット / 代入=共有 (JS・Swift 既定) | 代入される capture は共有セル経由で外側・兄弟と共有。別 `let` は独立 |
| FFI 構文 | `@extern(C) { ... }` ブロック | per-fn フラグまみれの旧構文を捨てて Rust の extern "C" {} を踏襲 |
| FFI 型カプセル化 | raw pointer / C-only 型はブロック内のみ書ける + 値を外に漏らせない | 「ブロックの内側だけが unsafe」という Rust と同じ思想 |
| プロジェクトファイル | `ilang.toml` (Cargo 風) | binding 配布のため最小限の `[deps]` だけ |

## ARC まわりの核心ルール (JIT)

- **caller 側で aliased な heap 引数を retain**、callee 側で param を関数出口で release (deinit の `this` は除外 — `release_object` が lifecycle を持つので二重 release で無限再帰)
- ブロック終了時に新規 heap binding を **LIFO 順** で release
- `let y = x` / 関数引数 / `obj.field = x` / `a[i] = x` で borrowed 元 (Var/Field/Index/This) なら retain (fresh 値はそのまま rc=1 を譲渡)
- `emit_bind_retain` ヘルパが bind 時 rc 調整を集約 (heap 一般則 + fresh strong → weak 変換時の特殊ケース含む)
- 代入上書きは **新値 retain → store → 旧値 release** の順
- ブロック / `__main` の **tail retain は aliased heap 値のときだけ**
- 文字列 fresh オペランドは concat / eq 後に release
- `release_object` は drop_fn 中 weak_rc を sentinel +1 (back-edge weak の早期解放回避)
- **クラス宣言は 2 段階**: `declare_class_name` で名前→id だけ登録 → `finalize_class_layout` でフィールド型解決

### Closure ARC
- closure 専用のヘッダ (`[rc | drop_fn | total_size]`) + 専用 retain/release helper
- 各 closure wrapper に対し heap captures を release する drop fn を自動生成 (`__drop_closure_<name>`)
- `JitTy::Fn` は `is_heap()` に含まれる → 既存の let-bind retain / scope release 経路で自動管理
- 入れ子 closure: 内側 closure の construct site が外側の `closure_capture_env` を見て env から load して再 capture

## 開発フロー

```sh
# 全テスト (cargo-nextest 経由、~30 秒)。`.cargo/config.toml` の
# alias で `cargo t` = `nextest run --workspace`、`cargo tci` =
# `--profile ci` (リトライ + fail-fast オフ)。設定本体は
# `.config/nextest.toml`。doctest は別途 `cargo test --doc` 必要
# (~20 秒)
~/.cargo/bin/cargo t

# cargo-nextest が無いホスト用フォールバック
~/.cargo/bin/cargo test --workspace

# REPL (let / fn / class が永続化)
~/.cargo/bin/cargo run -p ilang

# ファイル実行 (MIR JIT)
~/.cargo/bin/cargo run -p ilang -- run path.il

# AOT ビルド (Mach-O / ELF を吐く)
~/.cargo/bin/cargo run -p ilang -- build path.il -o ./out

# 1 つの fixture を直接実行
./target/debug/ilang run crates/ilang-cli/tests/programs/04_modules/extern_cstr_array.il

# SDL サンプル (要 SDL2 インストール: brew install sdl2 / apt install libsdl2-dev)
./target/debug/ilang run examples/sdl_breakout/main.il

# cocoa バインディングテスト (macOS のみ。非 macOS では skip)
#   - foundation: NSString / NSArray / NSDate / NSURL / NSData / 他
#                 38 fixtures, 645/1989 selectors (32%), 136/179 classes (76%)
#   - appkit    : NSWindow / NSButton / NSColor / NSBezierPath / 他
#                 11 fixtures, 169/508 selectors (33%), 44/53 classes (83%)
# `-- --nocapture` でカバレッジレポートを stdout に流す
~/.cargo/bin/cargo test --release -p ilang --test cocoa_foundation -- --nocapture
~/.cargo/bin/cargo test --release -p ilang --test cocoa_appkit -- --nocapture

# 個別 fixture を直接実行
./target/release/ilang run bindings/cocoa/foundation/test/strings_test.il
./target/release/ilang run bindings/cocoa/appkit/test/drawing_test.il
```

`source "$HOME/.cargo/env"` を使うと権限プロンプトが出る (settings.local.json の Bash allow が `Bash` 単独だと効かない)。**`~/.cargo/bin/cargo` を直接呼ぶこと**。

### scanner / parser ベンチ

`crates/ilang-parser/benches/scan_parse.rs` に criterion ベンチがある。stdlib / `tests/programs` 全体 / 全プログラム連結 の 3 コーパスを lex 単独・lex+parse の 2 段で計測する。

```sh
# ベースライン保存 (最適化前に1回)
~/.cargo/bin/cargo bench -p ilang-parser --bench scan_parse -- --save-baseline before

# 変更後の比較 (criterion が before との差分を出す)
~/.cargo/bin/cargo bench -p ilang-parser --bench scan_parse -- --baseline before

# 単一グループだけ走らせる例
~/.cargo/bin/cargo bench -p ilang-parser --bench scan_parse programs -- --baseline before
```

サンプル数や測定時間は `--sample-size 50 --warm-up-time 2 --measurement-time 5` 等で増やせる。デフォルトは短時間 (1秒ウォームアップ・3秒測定) なので、有意差判定が「noise threshold」になりがちな場合は増やす。

### type-check / MIR-lower ベンチ

`crates/ilang-mir/benches/check_lower.rs` に criterion ベンチがある。`tests/programs` の中で load+check+lower が成功する全プログラムを 1 ラウンドとして:

- `check_lower/check` — `ilang_types::check` のみ
- `check_lower/lower` — `ilang_types::check` + `ilang_mir::lower_program` (実パイプラインと同じ順序)

```sh
~/.cargo/bin/cargo bench -p ilang-mir --bench check_lower -- --save-baseline before
~/.cargo/bin/cargo bench -p ilang-mir --bench check_lower -- --baseline before
```

このベンチは 343 個の小プログラムを直列実行する都合上、scan_parse より run-to-run の variance が大きい。意味のある差分判定をしたいときは `--sample-size 50 --warm-up-time 3 --measurement-time 6` 程度を指定する。

### コミット方針
- 機能単位で 1 コミット
- メッセージは英語、末尾に `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`
- ユーザーが「コミットして」と言うまでコミットしない (頻出パターン)

### コードスタイル
- コメントは「なぜ」だけ。「なに」は書かない
- 公開 API のみ `pub use` で再エクスポート
- 警告ゼロを維持
- 役割別モジュール分割を維持 (`lib.rs` を肥大化させない)
- 大きな機能を追加するときは fixture を `tests/programs/` に必ず追加 (MIR JIT + AOT 両方で動くか自動検証されるため)

## 次の候補

ユーザーと相談して選ぶこと。

### A. 言語機能 (重要度高)
- **タプル** `(i64, string)` — 匿名 product 型。複数戻り値 / 一時的なペアに毎回クラスを定義するのが冗長
- **`?` 演算子** — `let v = parse(s)?` で `Result.err` を即 return。Result 連鎖が match ネストにならない
- **文字列補間** — `"hello, ${name}"`
- **Iterator プロトコル** — ユーザ型に `next()` を実装させて for-in に乗せる仕組み。ジェネレータの基礎
- **デフォルト引数 / 名前付き引数** (デフォルト引数は実装済み、名前付き呼び出しは未)

### B. FFI / バインディング配布
- **C ヘッダから .il を自動生成するミニ bindgen** — 現状は手書き。3 段階(手動 YAML / clang JSON AST → スクリプト変換 / libclang フル統合)を [docs/syntax.md](syntax.md) ではなく相談済みのチャットで議論済み。当面は `bindings/sdl2/` を手書きでメンテ
- **`bindings/libc/`** など他のライブラリも同形式で整備
- **`bindings/sdl2/`** の拡張: SDL_Image (PNG 読み込み) / SDL_ttf (フォント) / イベントの SDL_PollEvent 構造体対応(現状は SDL_GetKeyboardState ベースのポーリングだけ)

### C. 言語機能 (重要度中)
- 演算子オーバーロード (`class Vec2 { + (other: Vec2): Vec2 { ... } }`)
- Trait / Interface — type-by-shape 抽象化、継承と直交
- デストラクチャリング (`let (a, b) = pair`)
- async/await
- ジェネリック制約 (bounds)

### D. capability の enforce ← **言語ビジョンの核**
- 呼び出し側にも `@requires(...)` を要求するチェック
- `use http_client with cap(net = env.net)` のような import 構文
- クラスに capability を持たせる構文設計
- MIR を挟むタイミングで一緒に入れるのが自然 (関数呼び出しグラフが扱いやすい)

### E. JIT 補完
- 静的フィールドの string / object 対応
- ジェネリッククラスでの継承 / 静的メンバー / プロパティ
- **`&CONST` (トップレベル const のアドレス取得) の最適化**:
  現状の lowering (`crates/ilang-mir/src/lower/ops.rs`
  `lower_addr_of_decomposed`) は loader が demote した repl_slot
  経由で値をロードし、CRepr Object の場合のみポインタ値を再タグする
  形で `&IID_X` を通している。毎回 `__repl_load_slot` を呼ぶので、
  ホットパスでは無駄。.rodata 相当の静的データシンボルに焼いて
  `symbol_value` で参照する形にすれば呼び出しオーバーヘッドが消える。
  CRepr 以外 (i64/f64/string などのプリミティブ const) は現状未対応
  で `&` がエラーになる — 必要になったらスタックコピー経路も追加

### F. MIR まわりの拡張 (中間表現は導入済み)
AST → MIR (SSA + block-args) → Cranelift IR の経路は完成済み。次に乗せやすいもの:
- LLVM / wasm 等の別バックエンド (Cranelift 経路と並走できる)
- MIR レベルでの constant folding / dead code elim
- capability enforce の検査箇所として活用
- 軽量バイトコード VM (起動高速化やデバッガ統合の足場として)

## 会話のトーンと言語

- ユーザーへの返答は **日本語** (このファイルも日本語)
- コード/識別子/コミットメッセージは英語
- 提案する場合は 2-3 案のトレードオフを示し、推奨を 1 つ明示
- ユーザーが選んだ後は実装まで進める (確認は重要なところだけ)
- 大きな機能追加では「Phase A→B→C と段階的に」を提案するパターンが多い (継承、static、closure、FFI リファクタはこの形で着地済み)

## 既知の細かい落とし穴

- **JIT の内部 typecheck**: `jit_run_inner` の中で TypeChecker をもう一度動かす (`define_main` 内)。第 2 パス用の side table (`closure_wrapper_captures`, `loop_break_types`, `class_method_slots`, `class_vtable_lens`) を毎回最新に保つこと
- **Hoist pass は MIR mono の一部**: `crates/ilang-mir/src/monomorphize/hoist.rs::hoist_anon_fns` が FnExpr → Closure 変換を行う。`ExprKind::Closure` は typechecker からは `unreachable!` で除外され、hoist 後の MIR でのみ現れる
- **`ExprKind` を追加したら walker を全部更新**: monomorphize.rs に 6+ の walker (hoist_in_expr / scan_expr / subst_expr / rewrite_expr / walk_expr_children / map_expr_children + rewrite_calls_in_expr / rewrite_enum_refs_in_expr) があり、checker / mangle / loader / normalize にも match 漏れチェックが効く
- **AST の `is_override`**: `FnDecl.is_override` は override メソッドのときだけ `true`。クローンする箇所が monomorphize に多数あるので忘れずに `f.is_override` を伝播
- **`extends` 周りの class_signature**: parent をオーバーレイしてから子の declarations をマージ。`init` / `deinit` は per-class なので override 必須チェックの対象外 (特殊条件あり)
- **`docs/syntax.md` を最新に保つ**: 機能追加するたびに必ず更新する (ユーザーが頻繁に参照)
- **`@extern(C)` の synth パイプライン**: ブロック内の struct / fn / class は `synthesize_extern_c_classes` / `synthesize_extern_c_fns` で AST レベルでトップレベル相当に展開される。ここを通る `@lib` 付き fn には自動で `byValue` / `variadic` / `optional` 属性が付く。下流の native_extern.rs や class registration はこの synth 結果を読む
- **`@lib` fn のシンボル名と ilang 名**: ilang 名は loader の prefix で `module.fn_name` に化けるが、dlsym はオリジナルの C シンボルでなければならない。loader が `c_symbol` フィールドを自動でセット保存する仕組みになっている (`@symbol("...")` が明示されていなければ元の bare name)
- **FFI ヘルパー (`cstrFromString` 等) は loader の prefix から除外**: `prefix_block_calls` の `is_builtin_callee` 判定で組み込みヘルパーのリストを持つ。新ヘルパーを追加するときはここも更新
- **`ilang.toml` の検索**: CLI が entry file の親ディレクトリから上に辿って `ilang.toml` を探す。プロジェクトを横断する CLI 統合テストは現状ないので、変更時は `examples/sdl_breakout/` で動作確認するのが手軽

## WebGPU PoC (`examples/wgpu_triangle`) — 3 OS で動かす手順

ilang から `wgpu-native` (WebGPU の C 実装) を叩く PoC。**SDL2 で独立ウィンドウを作り、その上に wgpu サーフェスを張って WGSL シェーダで三角形を描く**。「シェーダを環境非依存(WGSL 1本)で 3 OS」を狙う検証。**macOS / Windows / Linux すべて実機確認済み**。Linux は Ubuntu (VirtualBox) の **Wayland セッション + lavapipe (ソフトウェア Vulkan)** で `PASS: presented 120 frames` を確認。サーフェスソースは `os.platform` で出し分ける(macOS=CAMetalLayer / Windows=HWND / Linux=X11 or Wayland を `info.subsystem` で分岐)。バインドは `bindings/wgpu/` (wgpu-native **v29.0.0.0** の `webgpu.h`/`wgpu.h` に固定)。

**Linux 実機検証で潰した本物の codegen バグ (x86_64 System V ABI)** — macOS aarch64 / Windows では別 ABI 経路のため顕在化していなかったもの。詳細は [`crates/ilang-mir-codegen/src/compile/abi.rs`](../crates/ilang-mir-codegen/src/compile/abi.rs):
- **>16 B の値渡し構造体引数**: 旧実装は「呼び出し側でコピーを作りそのポインタを渡す」隠しポインタ方式だった (AArch64 AAPCS64 / Win64 では正しい)。SysV では MEMORY クラスとしてスタックへ直接コピーするのが正で、Cranelift の `ArgumentPurpose::StructArgument` を使うよう修正 (`clif_signature_for`)。wgpu の `WGPURequestDeviceCallbackInfo` (40 B 値渡し) がこれで壊れ、`onDevice` のはずが `onAdapter` が呼ばれて device 取得が失敗していた。
- **構造体の戻り値**: SysV は戻り値レジスタが整数 2 本 (rax:rdx)・浮動 2 本 (xmm0:xmm1)。ilang ABI の 64 B chunk 上限のままだと 32 B 構造体が 4 戻り値になり Cranelift が拒否 (#9510)。戻り値専用キャップ `ret_chunk_max` / `struct_hfa_ret` を導入し、超過分は sret に落とす。
- **クロージャの bool 戻り値**: x86_64 の `setcc` は下位 1 バイトしか書かず上位はゴミ (aarch64 `cset` はゼロ化)。ランタイムが述語結果を i64 全幅で読んでいたため `filter`/`find`/`every`/`some` が誤動作。`call_predicate_1` で下位バイトのみ読むよう修正 (`crates/ilang-runtime/src/arrays.rs`)。
- **固定長配列 (`T[N]`) の解放カスケード**: 固定長配列はヘッダ無しのインラインデータなのに KIND_ARRAY 扱いで「ヒープ配列として free」していた → glibc が `munmap_chunk(): invalid pointer` で abort (macOS のアロケータは見逃す)。`kind_tag_of` / AOT `field_kind_tag` で `len: Some` を KIND_NONE に。enum ペイロードのカスケードタグも MIR 型ベースに変更 (`jit_setup.rs`)。

### 共通の準備

1. **ライブラリ取得**: `third_party/wgpu/fetch.sh` を実行 (`gh` CLI 必須)。OS/arch を自動判定して該当リリースを DL し、dylib/so/dll + ヘッダを `third_party/wgpu/<os-arch>/` に展開、巨大な `.a` と zip は削除する。バイナリは **未コミット** (`.gitignore` 済み)。
   - Windows で bash が無ければ git-bash で `fetch.sh` を実行するか、`gh release download v29.0.0.0 -R gfx-rs/wgpu-native -p "wgpu-windows-x86_64-msvc-release.zip"` を手動展開する。
2. **ライブラリ検索パス**を立てて実行 (バイナリは標準ディレクトリに無いため):
   - macOS:   `DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib ./target/debug/ilang run examples/wgpu_triangle/main.il`
   - Linux:   `LD_LIBRARY_PATH=third_party/wgpu/linux-x86_64/lib ./target/debug/ilang run examples/wgpu_triangle/main.il`
   - Windows: `wgpu_native.dll` (+ `SDL2.dll`) を PATH に通すか、entry / exe と同じディレクトリへ置く。`@extern(C, "wgpu_native")` の bare 名解決で拾われる。確認済みの起動例(git-bash):
     `PATH="$PWD/third_party/wgpu/windows-x86_64/lib:$PWD/target/release:$PATH" ./target/debug/ilang.exe run examples/wgpu_triangle/main.il`
     (SDL2.dll は `target/release` に同梱されている)。`fetch.sh` は git-bash/MSYS でも Windows asset を取得する。

### OS 別 SurfaceSource (3 OS すべて実装済み・実機確認済み)

`examples/wgpu_triangle/main.il` は `os.platform` を見て SurfaceSource を出し分ける。共通部 (adapter/device/pipeline/draw) は OS 非依存。3 種とも struct は `bindings/wgpu/mod.il` に定義済み。

- **macOS**: `SDL_Metal_CreateView` → `SDL_Metal_GetLayer` → `WGPUSurfaceSourceMetalLayer` (sType=4)。
- **Windows**: `SDL_GetWindowWMInfo` で HWND/HINSTANCE を取り `WGPUSurfaceSourceWindowsHWND` (sType=5) をチェイン。ウィンドウフラグは macOS だけ METAL を付け、それ以外は SHOWN のみ。
- **Linux**: `info.subsystem` で X11 (=2) / Wayland (=6) を分岐。X11 は `WGPUSurfaceSourceXlibWindow` (sType=6)、Wayland は `WGPUSurfaceSourceWaylandSurface` (sType=7)。**実機確認済み** (Wayland セッションで `info.subsystem==6` 経路を通過)。GPU 3D アクセラレーションが無い環境では wgpu が自動で lavapipe (ソフトウェア Vulkan) を選ぶ。

ネイティブハンドルは SDL から取得する (`bindings/sdl2/sdl_window.il`)。`SysWMinfo.version` を埋めてから `SDL_GetWindowWMInfo(win, &info)`、`info.subsystem` (windows=1 / x11=2 / wayland=6) と `info.handle1..4` を読む。各 SurfaceSource の handle 対応:

- **Windows** (`subsystem==1`): hwnd=handle1, hinstance=handle3
- **Linux/X11** (`subsystem==2`): display=handle1 (`*void`), window=handle2 (`u64` の XID)
- **Linux/Wayland** (`subsystem==6`): display=handle1 (`*void`), surface=handle2 (`*void`)

SDL の wayland/x11 の選択は環境変数や `SDL_VIDEODRIVER` に依存するので、コード側は `info.subsystem` を見て分岐している。

### この PoC で判明した ilang FFI の落とし穴 (踏み直さないこと)

- **`@extern(C)` struct の `@handle` フィールド**: 以前は値が壊れたが commit `8ecbef08` (`mir-codegen: store @handle struct fields as pointer-sized values`) で修正済み。**ハンドル型フィールドはそのまま `WGPUXxx` 型で宣言してよい** (`*void` 回避策は不要)。ハンドル配列 (`WGPUCommandBuffer[]`) の要素も正しく直列化される。
- **ハンドル型は必ず `@handle pub struct WGPUXxx {}` を宣言する**。戻り値で使うだけだと型は通っても `handle as i64` 等のキャストが「expected i64, got WGPUXxx」で落ちる。
- **構造体 out パラメータは `&local` で渡す** (値ローカル/`new` どちらでも可)。`@extern(C)` struct を `*T` 引数へ**直接**渡すのは**設計上禁止** (型エラー) で、アドレス取得を明示させる方針 (`ops.rs::assignable` のコメント参照)。配列だけは `T[] → *T` が暗黙。caps 取得・`getCurrentTexture` はこの `&` 方式で動く。
- **コマンドバッファ配列は `WGPUCommandBuffer[]` を渡す** (`let cmds: WGPUCommandBuffer[] = [cmd]; wgpuQueueSubmit(queue, 1, cmds)`)。`T[] → *T` 変換でポインタが渡る。バインドの `commands` 引数は `*WGPUCommandBuffer`。
- **値渡しの CallbackInfo + コールバックの `WGPUStringView`**: `wgpuInstanceRequestAdapter`/`RequestDevice` は CallbackInfo を**値渡し**、コールバックは `WGPUStringView` を**値渡し**で受ける。コールバックは `WGPUStringView` を **2つの i64 に展開** (`fn(i32, i64, i64, i64, i64, i64)`) して受けると ABI が合う。wgpu-native はコールバックを同期的に発火するので `wgpuInstanceProcessEvents` 後にスロットから読めばよい。
- **`WGPUStringView.data` は `*void`**。`cstrFromString(s) as *void` を入れ、`length` は `0xFFFFFFFFFFFFFFFF`(=WGPU_STRLEN, SIZE_MAX) にすると wgpu 側で strlen される。
- **enum 値はヘッダ準拠の flat 値**でOK (`fmt`/`alpha`/`present` は `wgpuSurfaceGetCapabilities` の実値を使うのが堅い)。`WGPUTextureUsage`/`WGPUColorWriteMask` は **64bit (WGPUFlags)** なので struct フィールドは `u64`。
- **ドローアブル取得**: `wgpuSurfaceGetCurrentTexture` はウィンドウが**画面に合成されるまで** texture=null を返す。`SDL_ShowWindow` + `SDL_RaiseWindow` でウィンドウを前面化し、`SDL_PumpEvents` を回しつつ **texture が取れるまでリトライ**する (PoC のループ参照)。

### 既知の未解決/保留

- 取得ライブラリと同梱ヘッダで **`WGPUSurfaceGetCurrentTextureStatus` の値が食い違って見える瞬間がある** (texture 取得失敗時に `0x00030001` が観測された)。texture 取得成功時は `status=1` で header と一致するので、**status の数値で判定せず `texture != 0` で判定**している。
- **ライブラリ取得は `gh` 必須だが、無ければ `curl -fSL https://github.com/gfx-rs/wgpu-native/releases/download/v29.0.0.0/wgpu-linux-x86_64-release.zip` で直接 DL → `unzip` でも可** (`fetch.sh` は `gh` 前提)。
- formatter (`ilang-lsp`) はバッククォート テンプレートリテラルを `TmplStart` / `TmplLit` / `TmplEnd` の 3 トークンに分けて lex する。以前は content と閉じバッククォートの間に空白が入って WGSL 文字列を壊していた(`main.il` で `formatter_preserves_lexability_on_corpus` が落ちていた)。`needs_space` でこの 3 トークン同士を密着させて修正済み。

## ラウンド 119 — enum メソッド未対応(機能ギャップ)/ 双方向 weak DLL を pin(clean)

- **enum メソッドは未対応**。`enum E { ... fn area(): f64 { ... } }` は `parse error: unexpected token Ident("area") (expected ',' or newline between variants)` になる。enum 本体はバリアント宣言のみで、メソッド定義構文は持たない。クラッシュではなく明確な parse error なので、バグではなく機能ギャップ。
- **双方向連結リスト(`next` strong + `prev` weak)** を突いたが健全。前方走査(strong `next`)=6、後方走査(weak `prev` を `.get()` で upgrade)=6、teardown で全 3 ノードが deinit(weak 後方鎖がサイクルを断つのでリークなし)。既存 weak fixture は親子(tree)構造のみで、両方向に走査する線形リストは未カバーだったため `05_edge_cases/weak_doubly_linked_list.il` を追加(JIT/AOT 両方通過)。

## ラウンド 120 — クロージャ ARC を多角的に確認 / ループ反復ごとの新束縛キャプチャを pin(clean)

- クロージャの基本〜ARC を一通り突いたが全て健全。(1) heap オブジェクトをキャプチャした closure は scope を越えてそれを生存させ、closure drop 時に 1 回だけ deinit。(2) 2 つの closure が 1 つのキャプチャを共有すると同一オブジェクトを参照し deinit も 1 回。(3) 返却クロージャ・可変キャプチャ・per-call snapshot などは既存 fixture(`closure_capture_runs_deinit`, `leak_closure_heap_capture`, `closures_array_share_cell` 他多数)で網羅済み。
- 唯一の未 pin は **ループ本体の `let` が反復ごとに新しいセルになる** ケース。`while` 内で `let x = i` をキャプチャした closure を配列に push すると、3 つの closure は `0,1,2` を返す(全て `2` ではない)。共有セル fixture(`closures_array_share_cell.il`)とは逆の意味論なので `10_closures_arc/loop_per_iteration_capture.il` を追加(JIT/AOT 両方通過)。

## ラウンド 121 — float `toString` の round-trip 精度・負ゼロを `float_to_string.il` に追記(clean)

- float の `toString` 整形を突いたが値はすべて正確。整形は **科学記法を一切使わず常に完全展開** する設計で、`1.0e23` → `100000000000000000000000`、`1.0e308` → 309 桁、`1.0e-300` → 0.000…0001 となる。これは冗長だが round-trip 上は正しい(最近接 f64 にパースし直せる最短表現の一つ)。整数 MIN・`Infinity`/`-Infinity`/`NaN` も妥当。バグではなく整形の冗長さ。
- 既存 `float_to_string.il` は 1e20 冗長表示・特殊値・f32 を網羅済み。未カバーだった **最短 round-trip 精度**(`0.1 + 0.2` → `0.30000000000000004`、`1.0/3.0` → 17 桁)、**負ゼロの符号脱落**(`-0.0` → `"0.0"`、JS 一致)、**2^53+1 の丸め**(→ `9007199254740992.0`)を同 fixture に追記(JIT/AOT 両方通過)。

## ラウンド 122 — 文字列はコードポイント基準を確認 / `string_edge.il` に絵文字 slice・indexOf・全置換を追記(clean)

- 文字列メソッド(`charAt`/`slice`/`indexOf`/`replace`/`split`/`trim` 他)を突いたが全て健全。インデックスは byte でも UTF-16 code unit でもなく **Unicode コードポイント基準**(runtime は `s.chars().collect()`)。`"a😀b".slice(0,2)` は `"a😀"` を返し、サロゲートを割らない(JS の `slice` は lone surrogate を生む)。`indexOf("b")` はコードポイントの 2(UTF-16 の 3 ではない)。
- 範囲外・負・空も妥当。`slice` の負インデックスは **0 にクランプ**(末尾基準ではない、`start.max(0)`、意図的設計)。`replace` は **全置換**(Rust `str::replace` と同じ。JS の最初のみとは異なる)。
- 既存 `string_edge.il` は length のコードポイント計数・負クランプ・多バイト slice を網羅済みだったが、(1)絵文字を跨ぐ slice/charAt、(2)コードポイント基準 indexOf、(3)全置換、が未 pin(replace は fixture コメントが「runtime 依存」と pin を回避していた)。これら 3 点を同 fixture に追記(JIT/AOT 両方通過)。

## ラウンド 123 — 整数除算/剰余を確認 / `i64::MIN / -1` のラップを `negative_division.il` に追記(clean)

- 符号付き整数の除算/剰余を突いたが健全。**ゼロ方向の切り捨て**(C/Rust 流。`-7/2 == -3`、`-7%2 == -1`)で恒等式 `(a/b)*b + a%b == a` も成立。ゼロ除算/剰余は `panic: division by zero` / `modulo by zero` で rc=1(JIT/AOT とも制御されたトラップ、SIGFPE ではない)。これらは既存 fixture(`division_by_zero_int`, `modulo_by_zero`, `const_div_zero_error` 他)で網羅済み。
- 未 pin だったのは **`i64::MIN / -1` のオーバーフロー**。数学的には 2^63 で表現不能だが SIGFPE せず **i64::MIN にラップ**(x86 の生 `idiv` なら #DE になる箇所)。`% -1` は 0。JIT/AOT 一致。`negative_division.il` に追記(変数経由で const-fold を避け runtime パスを通す)。

## ラウンド 124 — Map の挿入順序を pin / index-read 欠損 panic と `.get()` optional を確認(clean)

- Map を突いたが健全。`m[key]`(index 読み)は欠損キーで `panic: key not found in map`(「存在前提」アクセサ、既存 `map_index_missing_key_panics.il` で pin 済み)、`m.get(key)` は optional を返す安全版。size/has/delete も正常。
- **挿入順序**: 実装は `order: Vec<i64>` で挿入順を明示的に保持し、runtime コメントに「map iteration は deterministic で JS Map 準拠(上書きは位置維持・削除はスロット除去)」と契約として明記(HashMap のランダム順がプログラムに漏れる非決定性バグの修正として導入済み)。しかし `map_get_keys_values.il` は「order is unspecified」として membership のみ assert していた。
- これは設計判断ではなく文書化済み契約の pin なので `03_collections/map_insertion_order.il` を追加。keys()/values()/entries() が同一の挿入順を辿ること、上書きで位置が維持されること、削除でそのスロットだけ除去されることを固定(JIT/AOT 両方通過)。

## ラウンド 125 — 配列メソッドを確認 / sort の非変更(receiver 不変)を pin(clean)

- 配列を突いたが健全。OOB/負インデックスの index 読みは `panic: index out of bounds`(制御されたトラップ)。`pop`/`shift`/`find` は optional を返し空配列でも安全(none)。`map`/`filter` は新配列。`reduce` は未実装(メソッド一覧: concat/fill/filter/find/findIndex/forEach/includes/indexOf/join/map/pop/push/remove/removeAt/reverse/shift/slice/some/sort/unshift)。
- **sort/reverse は非変更**(`docs/syntax.md:1248`「reverse / map / filter / sort は receiver を変更せず fresh copy を返す」)。`u.sort(cmp)` は元配列を変えず、ソート済みの新配列を返す(返り値を使わないと no-op に見える footgun)。reverse の非変更は `array_reverse_independence.il` で pin 済みだが、**sort の receiver 不変は未 pin**(既存 fixture はソート結果のみ検証)。
- `03_collections/array_sort_independence.il` を追加。sort 返り値が昇順/降順に整列すること、receiver が元の順序を保つこと、2 つのバッファが独立(コピーへの書き込みが元に波及しない)ことを固定(JIT/AOT 両方通過)。

## ラウンド 126 — match/range/再帰 enum を確認 / int 網羅性の value-range 非考慮を pin(clean)

- match/パターンを突いたが健全。再帰 enum(`node: (Tree, Tree)`)+再帰 match は `enum_recursive_payload.il` で sum/depth/count まで網羅済み。整数 range パターン(半開 `..`/閉 `..=`/開放端 `..N`/`N..`、符号付き境界)は `match_int_range.il` で網羅済み。string/bool match も既存 fixture でカバー。
- **機能ギャップ(clean parse error)**: (1) match arm の or-pattern `1 | 2 | 3` 未対応、(2) primitive scrutinee で bare identifier の束縛不可(catch-all は `_` + 元変数参照)、(3) enum variant の `;` 一行区切り不可(改行必須)。いずれもクラッシュではなくバグではない。
- 未 pin だったのは **整数網羅性の value-range 非考慮**。`..0 / 0 / 1..` は数学的に全 i64 を覆うが、checker は range 被覆を推論せず `_` を要求し `non-exhaustive match on i64` を出す(bool のみ値空間を列挙する)。expect-error fixture `match_int_range_needs_wildcard.il` を追加して保守的ルールを固定。

## ラウンド 127 — Set を確認(既存網羅済み)/ ラウンド124 の重複 fixture を削除

- Set を突いたが健全。dedup・has・delete・挿入順反復(`312`)・union/intersection/difference・object 要素・float 要素まで、既存 `map_set_insertion_order.il`(commit a4572ede)が網羅済み。object 要素 Set は `@derive(Eq, Hash)` または `equals`/`hashCode` 宣言を要求する明確なエラー。Set に新規 pin は不要。
- **確認不足の訂正**: ラウンド124 で追加した `map_insertion_order.il` は既存 `map_set_insertion_order.il` と重複していた(後者が Map/Set の挿入順を上書き/削除+再挿入/keys-values-entries/object/float まで網羅)。fixture 検索時に既存ファイルを見落としていた。挿入順はコメントに「JS semantics, user decision」とあり既にユーザー判断で確定済みの仕様。ユーザー判断により重複 fixture を削除。

## ラウンド 128 — plain interface 継承を実装(checker/MIR 不整合バグの修正)

- バグ: `interface B: A`(plain・非@com)で継承メソッド呼び出しが型チェックを通過するのに MIR lowering で `interface B has no method foo` という内部エラーを露出。型チェッカ(`calls.rs:824-836`)は parent チェーンを辿って解決するが、MIR(`object.rs lower_iface_dispatch`)が leaf interface 自身のスロットしか引かず、parent を辿らなかった。@com 版は parent を辿るので動いていた。
- ユーザー判断: **MIR で継承を実装**。親のグローバルスロットを再利用する方針:
  - `iface_parents: HashMap<Symbol,Symbol>` を LoweringState に追加、interface 登録パスで `i.parent` から構築、body_cx へ配線。
  - `lower_iface_dispatch` を `(ifn,method)` 未発見時に parent チェーンを辿り、祖先の slot/sig を使うよう変更。
  - クラスの vtable 登録(`decl/class.rs`)で `declared_ifaces` を祖先 interface まで展開し、継承メソッドを祖先スロットに登録。
- checker 側も整合させた:
  - `class_implements`(`utils.rs`)を `iface_inherits_from` 経由で interface 親チェーンも辿るよう拡張(推移的準拠 `C: B, B: A ⟹ C: A`、代入可能性)。
  - 準拠チェック(`decls.rs check_class`)の `declared_ifaces` を祖先まで展開し、継承メソッド未実装を `does not implement A.foo` で弾く(空 vtable スロットへの dispatch を防ぐ)。
- 確認: B/C/A 各 receiver からの own+継承メソッド呼び出し、3段継承(A←B←C)、interface 型配列の動的 dispatch、root interface への推移的代入、準拠漏れエラー、JIT/AOT すべて正常。workspace 539/539 PASS。fixture 追加: `09_subtyping/interface_inheritance.il`(正常系)、`interface_inheritance_missing_method_error.il`(expect-error)。

## ラウンド 129 — generic 自由関数を確認 / ネスト合成を pin(clean)

- generic 自由関数を突いたが健全。型推論・モノモーフィ化・タプル返却・分割代入すべて動作。明示的型引数 `identity<i64>(5)` は未対応(`<` が比較として解釈される機能ギャップ、メソッドの `obj.m<T>()` と同様)。interface のデフォルトメソッド本体・generic 境界 `<T: Sized>` も未対応(clean parse error、機能ギャップ)。
- 未 pin だったのは **ユーザー generic 自由関数同士のネスト合成**。`firstOf(wrap(99))` のように一方の generic 呼び出しの結果を別の generic 呼び出しの引数に直接渡し、両方の型引数を推論のみで解く形。既存 fixture は enum/Result コンストラクタの入れ子が中心で、generic-fn-into-generic-fn は無かった。`05_edge_cases/generic_fn_nested_composition.il` を追加(3段ネスト・タプル返却 generic・i64/string 両方、JIT/AOT 通過)。
