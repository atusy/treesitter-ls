# 変更ルール（復唱）
- 変更開始前に変更ルールを復唱すること
- step-by-stepで動作可能な範囲で`git commit`しながら変更を進める
- `git commit`前には必ず `make test format lint`を実行し問題をすべて解決する
- README.md/CONTRIBUTING.mdも随時更新する
- 進捗や気付きを逐次`plan.md`に記載する

# 背景と目的
- treesitter-lsのCode Action「Inspect token」にTree-sitterのインジェクション情報を表示し、利用者が対象トークンの言語解析フローを一目で把握できるようにする。

# 変更方針
- 未公開プロジェクトのため後方互換性を気にせず、必要であれば大規模なリファクタリングも恐れず進める。
- 既存コードへの忖度は不要。TDDサイクルで必要最小限の実装を重ね、常にクリーンな構造を保つ。
- Kent BeckのTidy Firstを意識し、構造的変更と振る舞い変更を明確に分離する。

# 変更計画
## 全体像
1. 現状のInspect tokenコードアクションの構造と、Tree-sitterインジェクション情報の取得経路を把握する。
2. インジェクションチェーンをInspect token出力に表示するテストを追加し、失敗を確認する（Red）。
3. テストが要求する最小限のインジェクション情報取得・表示ロジックを実装し、テストを通す（Green）。
4. 重複や構造上の改善点があれば、テストを維持したままリファクタリングする（Refactor）。
5. README.mdなどドキュメントにInspect tokenの新機能を追記する。

## 段階的詳細
### Step 1: 現行実装の把握
- 作業内容
  - コードアクションのエントリーポイント（`lsp`配下）とInspect tokenの具体的な実装箇所を洗い出す。
  - Inspect tokenで利用しているサービス層（`workspace`や`analysis`など）のフローを整理する。
  - Tree-sitterのインジェクション情報をどこで扱っているか、既存のテストやドキュメントも含めて確認する。
  - これらの調査結果を`plan.md`の「変更中に得た気付き」に追記し、影響範囲と前提条件を明文化する。
- DoD
  - 上記の調査結果が`plan.md`に記載され、次のStepで参照できる状態になっている。

### Step 2: 振る舞いを規定するテストを追加（Red）
- 作業内容
  - Inspect tokenのレスポンスを検証する既存テストを探す。見つからない場合は最適な場所（単体 or 結合テスト）を選定し理由を記録する。
  - 「インジェクションチェーンが表示される」ことを最小の観点で保証するテストケースを追加する。
  - テスト名称は振る舞いを説明する文章にする。
  - テストが失敗することを確認し、失敗ログの要点を`plan.md`にメモする。
- DoD
  - 新規テストがRedである。
  - 失敗理由がインジェクション表示未実装に起因することが明確。

### Step 3: 最小実装でテストを通す（Green）
- 作業内容
  - Redテストで要求されたインジェクションチェーン表示を成立させる最小限の実装を行う。
  - テストを通すために必要なデータ取得ロジックを既存の階層にうまく配置する。設計上の決定は`plan.md`に記録する。
  - `make test format lint`を実行し、すべて成功させる。
- DoD
  - `make test format lint`が成功。
  - 新規テストがGreen。
  - 実装内容と判断理由が`plan.md`にメモされている。

### Step 4: 必要なリファクタリング（Refactor）
- 作業内容
  - 重複除去、命名改善、責務分離など、構造改善だけを目的とした変更を検討する。
  - 変更の前後で`make test format lint`を実行し、挙動が維持されていることを確認する。
  - 単一言語スタックでも Inspect token に Languages が出力されることをテストで保証する。
  - 実施したリファクタリングパターンを`plan.md`に記録する。
- DoD
  - リファクタリング後も`make test format lint`が成功。
  - 実施したリファクタリング内容が`plan.md`に記録されている。

### Step 5: ドキュメント更新
- 作業内容
  - README.mdやCONTRIBUTING.mdにInspect tokenのインジェクション表示機能を追記し、利用者が気付けるようにする。
  - ドキュメント変更後も`make test format lint`を通す。
- DoD
  - ドキュメント更新が反映済み。
  - `make test format lint`が成功。
  - 更新内容の概要が`plan.md`に残っている。

# 進捗状況
- Step 1: 完了（Inspect token経路・インジェクション周辺の現状を整理）
- Step 2: 完了（Redテスト追加済み）
- Step 3: 完了（インジェクション表示実装・Green）
- Step 4: 完了（単一言語時も表示）
- Step 5: 未着手

# 変更中に得た気付き
- 作業着手前に変更ルール（復唱、段階コミット、`make test format lint`順守、逐次記録、ドキュメント更新）を確認済み。
- Step 2: `inspect_token_should_display_language_injection_chain` を追加し、`create_inspect_token_action` に言語スタック引数を導入。`make test` 実行で 期待通り Red（Languages: markdown -> rustが未出力）。
- 単一言語（例: markdown ヘッダ）でも言語情報が欠落する事象を利用者から報告。Redテスト追加で再現する。
- Step 4: `inspect_token_should_display_root_language_when_no_injections` で Red -> Green。`create_inspect_token_action` が capture_context を fallback として使用し root 言語を出力するよう拡張。
- `make test`, `make format`, `make lint` を通過して警告なしを確認済み。
- 言語スタックは root layer + カーソル位置にマッチする injection layer を抽出し、2段以上ある場合のみ Inspect token に `Languages: ...` を表示。
- Step 3: `create_inspect_token_action` にインジェクションチェーンの整形を実装し、`handle_code_actions`/`lsp_impl::code_action` から言語スタックとキャプチャコンテキストを渡すよう拡張。
- Inspect tokenコードアクションは `src/analysis/refactor.rs` の `create_inspect_token_action`/`handle_code_actions` で生成され、LSP層 `src/lsp/lsp_impl.rs` の `code_action` から root layer の tree とハイライト/ローカルクエリ、キャプチャマッピングを受け取っている。
- 現状は `doc.layers().root_layer()` の tree しか参照しておらず、`DocumentView::injection_layers` や `LayerManager::get_layer_at_offset` などのインジェクション層情報は未活用。
- `LayerManager`／`LanguageLayer` にはインジェクション層APIが揃っているが、`DocumentStore` からの構築フローやコードアクション側での利用は未実装。
- Inspect token を検証する既存テストは `tests/` 配下に存在せず、新機能の振る舞いをカバーするテスト追加が必要。
- `language::coordinator` で `injections.scm` が `QueryStore` にロードされているものの、解析系で言語チェーン表示に活用されていない。

