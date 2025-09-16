# Refactoring Plan

> この計画は進行にあわせて逐次更新する。

## 変更の背景と目的
- LSP層と内部モジュールで`tower_lsp::lsp_types`と独自`domain`型が重複し、変換コストと保守性の低下を招いている。
- ドキュメント・ワークスペース層がLSP固有型に依存しており、再利用性を阻害している。
- 解析ロジックが`runtime`モジュール内部に直接依存し、境界の明確さとテスト容易性が損なわれている。

## 変更方針
- プロジェクトは未公開のため後方互換性を気にせず大胆に再設計する。
- 既存コードへの忖度は不要。最も合理的な構造に向けて壊すべきところは壊す。
- `core`のような役割が曖昧な曖昧な層は作らず、責務を明確に分割する。

## 変更計画
1. 現行構造の棚卸し
   - `domain`と`tower_lsp::lsp_types`の境界・変換箇所を全探索しリストアップする。
   - ドキュメント/ワークスペース層の`Url`・`SemanticTokens`等、LSP依存の痕跡を洗い出す。
   - `analysis`→`runtime`の依存ポイントと利用関数を整理し、抽象化が必要な範囲を特定する。
   - インプット: 既存コード、調査ログ。アウトプット: 後続タスクで参照する棚卸しメモ。
2. 目標ディレクトリ構造に沿った責務マッピング
   - 各既存モジュールを「残す」「移動する」「再設計する」に分類し、理由と移行先を明記する。
   - `application`層に収容すべきユースケース（例: 文書のパース要求、トークン配信）を文章で定義する。
   - 新しい依存方向（上位→下位）のルールを図または箇条書きでまとめる。
3. 型境界の再定義計画
   - `domain`で保持するモデル一覧と、LSP層にのみ残すべき型を決定する。
   - 変換責務を担うモジュール（例: `lsp/protocol_adapter`）の役割と入出力を文書化する。
   - LSP依存のデータを内部へ渡さないためのガイドライン（引数・戻り値の原則）を書き下す。
4. ドキュメント／ワークスペース層の再設計方針
   - LSP非依存化に向けたストレージAPIの一覧と期待する振る舞いを定義する。
   - `workspace`が公開する操作（例: 文書登録、解析実行、設定ロード）をAPI仕様として列挙し、内部構造への直接アクセス禁止を明文化する。
   - 並行性やロックの取り扱いポリシーを整理し、共有メモリの扱い方針を決める。
5. 解析モジュールの依存抽象化
   - `analysis`が必要とする`runtime`機能をインターフェース化し、必要なメソッドシグネチャと契約（例: 戻り値、エラーハンドリング）を文書化する。
   - 単体テスト観点からモック差し替え方法を想定し、テスト計画メモにまとめる。
6. ドキュメント更新計画
   - README/CONTRIBUTINGに追記すべき内容（ディレクトリ責務表、開発フロー、テストルール）を箇条書きにする。
   - リファクタリング進行中の読者向けに「移行ガイド」節のラフ構成を作成する。
7. スケジュールとマイルストーン設定
   - 各ステップの完了条件とチェックポイントを定義し、担当者と目安日程を割り当てる。
   - 進行管理用に必要なトラッキング（Issue、PRテンプレート等）を決める。


## 進捗状況
- [x] Step 1: 現行構造の棚卸し
- [x] Step 2: 目標ディレクトリ構造に沿った責務マッピング
- [x] Step 3: 型境界の再定義計画
- [x] Step 4: ドキュメント／ワークスペース層の再設計方針
- [x] Step 5: 解析モジュールの依存抽象化
- [x] Step 6: ドキュメント更新計画
- [x] Step 7: スケジュールとマイルストーン設定

## 変更中に得た気付き
- domain層とLSP層の二重定義は`src/lsp/lsp_impl.rs:27-194`の変換群が示す通り、メンテナンスコストの主因になっている。
- `workspace`が`tower_lsp::lsp_types::WorkspaceEdit`等へ依存しており、ストア系APIの抽象度が低いことが確認できた。
- `error.rs` はドメイン共通エラーとして再編する想定だが、LSP 向けエラー表現との切り分けが必要。
- URL 型は `url::Url` に統一すれば、LSP 層の `tower_lsp::Url` との二重管理が解消できる。
- ドキュメント層は `DocumentRepository` として公開すれば、`DashMap` 等の実装詳細を外部に漏らさずに済む。
- `analysis` からは `AnalysisRuntime` トレイト越しに runtime を呼ぶ設計にするとテストダブルの導入が容易になる。

## 変更ルール
これらのルールを順守することをここに再確認する。
- ステップごとに動作可能な状態で`git commit`する。
- `git commit`前には必ず `make test format lint` を実行し、問題を解消する。
- README.md と CONTRIBUTING.md は変更内容に合わせて随時更新する。
- 進捗や気付きを`__ignored/refactoring_plan.md`に逐次追記する。

## 目指すディレクトリ構造
- `lsp/` : LSPプロトコル入出力と内部表現の橋渡しのみを扱う終端レイヤー。
- `application/`(仮) : LSP以外のフロントエンドが載っても使えるユースケース単位のサービス群。
- `domain/` : プロトコルに依存しない純粋なドメインモデルとユースケース境界を定義。
- `infrastructure/`
    - `runtime/` : tree-sitter関連のロード・クエリ・パーサープールなど外部システム連携。
    - `document/` : 文章とレイヤー管理の永続化・キャッシュ管理。
    - `config/` : 設定ファイルやマッピングのロードと正規化。
- `analysis/` : ドメインモデルを入力とした解析ロジック。runtime依存は抽象化を介して注入。
- `workspace/` : application層から呼び出される集約ルート。内部構造は隠蔽し、明確な操作APIのみ公開。
- `text/` : 文字列/位置変換などの純粋なユーティリティ。副作用を持たない。

> 仮称のディレクトリ名は実装時に再検討するが、目的は各層の責務を物理ディレクトリで明示することにある。


## ステップログ

### Step 1 現行構造の棚卸し
- `src/domain/mod.rs:3-167` で LSP の Position/Range/CodeAction などを再実装している。
- `src/lsp/lsp_impl.rs:27-194` が domain と `tower_lsp::lsp_types` 間の変換関数群を大量に保持。
- `src/document/model.rs:1-30` と `src/document/store.rs:1-40` が `tower_lsp::lsp_types::SemanticTokens` や `Url` を内部保存。
- `src/workspace/mod.rs:9-90` が `tower_lsp::lsp_types::{SemanticTokens, Url}` を公開 API で露出。
- `src/analysis/semantic.rs:122-168` と `src/analysis/refactor.rs:32-55` が `crate::runtime::filter_captures` へ直接依存。


### Step 2 目標ディレクトリ構造に沿った責務マッピング
- `src/lsp` → `lsp/`: プロトコル境界に専念させ、他層の実装を参照しない。
- `src/workspace` → `application/workspace_service`: アプリケーション層のファサードとして公開 API のみ提供。
- `src/analysis` → `analysis/`: 現在の配置を踏襲しつつ、runtime 依存は抽象化経由に置換。
- `src/runtime` → `infrastructure/runtime/`: tree-sitter 管理とクエリロードを担当。
- `src/document` → `infrastructure/document/`: LSP 依存型を排除したストレージ API に置換。
- `src/config` → `infrastructure/config/`: 設定ロードと正規化を担う。
- `src/text` → `text/`: 文字列・位置変換ユーティリティとして共通利用。
- `src/domain` → `domain/`: プロトコル非依存なモデルを揃え、LSP固有要素は `lsp/` に移す。
- `src/error.rs` → `domain/errors.rs` (仮): 共通エラー型として格納し、LSP 層では必要に応じてラップする。
- `src/bin/main.rs` は `bin/` に留めつつ、初期化時に `application` 層のサービスファクトリを呼ぶ構成へ改める。
- 新設: `application/services/` に文書管理・解析要求を束ねるサービスを追加し、LSP 以外のエントリーポイントでも再利用可能とする。
- 新設: `lsp/protocol_adapter.rs` (仮) に型変換・メッセージ組み立て機構を閉じ込める。


### Step 3 型境界の再定義計画
- ドメインモデルとして維持: `Position`, `Range`, `SelectionRange`, `SemanticToken{,s}`, `DefinitionResponse`, `WorkspaceEdit` などは `domain/` に残し、LSP 固有のフィールド名や union 型は避ける。
- LSP 層に閉じ込める型: `tower_lsp::lsp_types::*` 全般、`InitializeParams` や `SemanticTokensFullDeltaResult` などのプロトコル仕様は `lsp/protocol_adapter` 経由で扱う。
- 変換ポイント: `lsp/protocol_adapter.rs` に `from_domain_*` / `into_domain_*` の双方向変換を用意し、LSP 層以外では `tower_lsp` を `use` しないルールとする。
- `url::Url` を標準 URL 型として採用し、ドキュメント層では `tower_lsp::Url` を保持しない。LSP 層では `Url::parse` / `Url::from` の橋渡しを担保する。
- ドメインエラー型 `LspError` は `domain::errors::WorkspaceError`（仮）に改名し、LSP レベルではこれを `tower_lsp::jsonrpc::Error` にマッピングする責務を持たせる。
- API ガイドライン: アプリケーション層以下の public 関数は引数・戻り値で `tower_lsp` を使用禁止とし、代わりに domain 型または Rust 標準型へ統一する。


### Step 4 ドキュメント／ワークスペース層の再設計方針
- `infrastructure/document` に `DocumentRepository` トレイトを定義: `upsert(DocumentId, DocumentInput)`, `get(DocumentId) -> DocumentSnapshot`, `apply_tree(DocumentId, ParsedTree)` 等を提供。
- `DocumentSnapshot` には `text`, `tree`, `language_id`, `semantic_tokens` を domain 型で保持させ、LSP 型や `tower_lsp` の列挙を排除。
- `infrastructure/document` 内部では `DashMap` を継続利用しつつ、公開 API では `Arc` + 内部ロックを隠蔽した構造体を返す。
- `application/workspace_service` では公開 API として `open_document`, `update_document`, `close_document`, `parse_document`, `request_semantic_tokens`, `apply_settings`, `lookup_language` を定義。
- `workspace_service` 内部では `RuntimeCoordinator` と `DocumentRepository` を注入し、直接アクセスを禁止。
- ルートパス管理は `workspace_service` に保持させ、設定ロード時に `infrastructure/config` を呼び出すフローを明文化。
- 並行性指針: `DocumentRepository` は内部で `DashMap`、`workspace_service` は `RwLock` で設定情報をガードし、ロック獲得失敗時は `domain::errors::WorkspaceError::LockPoisoned` を返す。
- セマンティックトークンキャッシュの更新は `workspace_service` が制御し、ドキュメント層はキャッシュの保存と取得に専念する。


### Step 5 解析モジュールの依存抽象化
- `analysis` から参照する runtime 機能を洗い出し、`AnalysisRuntime` トレイト（仮）を `analysis` 内に定義する。
  - `fn has_queries(&self, language: &str) -> bool`
  - `fn highlight_query(&self, language: &str) -> Option<Arc<Query>>`
  - `fn locals_query(&self, language: &str) -> Option<Arc<Query>>`
  - `fn capture_mappings(&self) -> CaptureMappings`
- `analysis` の各ハンドラには `impl AnalysisRuntime` を受け取るように変更し、直接 `crate::runtime::*` を `use` しない。
- `runtime` 側では `RuntimeCoordinator` に `impl AnalysisRuntime for RuntimeCoordinator` を実装し、既存の関数を委譲。
- `filter_captures` など共通ユーティリティは `analysis::query_utils` に再配置し、runtime 層からは廃止。
- テスト用モックとして `MockRuntime` を `analysis` のテストモジュールに用意し、`highlight_query` 等にフェイクデータを提供する。
- `analysis` API では `AnalysisRuntime` をジェネリックにとらえ、`&dyn AnalysisRuntime` での DI を前提とした関数シグネチャを用意する。


### Step 6 ドキュメント更新計画
- README.md
  - 新ディレクトリ構成図と層の責務表を追加。
  - 開発フローに「LSP 層は protocol_adapter 経由でのみ内部と通信する」旨を明記。
  - `make test format lint` を推奨する開発サイクルを更新。
- CONTRIBUTING.md
  - コミットルール: ステップ単位でのコミットと事前テスト実行を必須化。
  - 依存方向 (application → domain → infrastructure → text) を図解で提示。
  - テスト方針: analysis は mock runtime を使用し、integration テストは application 層経由で行う説明を追記。
- 新設ドキュメント
  - `docs/migration.md` (仮) に旧構造から新構造への移行ガイド (API 変更点、型の移動、ディレクトリ の対応表) を掲載。
  - `docs/runtime_adapters.md` (仮) に `AnalysisRuntime` 等のトレイトと実装の関係を整理。


### Step 7 スケジュールとマイルストーン設定
- Week 1: Step 1〜3 の調査・設計を完了。成果物: 棚卸しメモ、責務マッピング図、型ガイドライン文書。
- Week 2: Step 4〜5 の基盤リファクタリング実装とユニットテスト整備。成果物: 新しい `DocumentRepository` 実装、`AnalysisRuntime` トレイト。
- Week 3: Step 6 ドキュメント整備と `docs/migration.md` の初稿作成。README/CONTRIBUTING 更新を含む。
- Week 4: 統合テストおよび最終レビュー。マイルストーン: `application/workspace_service` API 固定、LSP プロトコル動作確認、移行ガイド v1.0 公開。
- 担当割り当て例
  - Domain/API 設計: Aさん
  - Runtime/Infrastructure: Bさん
  - Documentation & Release: Cさん
- トラッキング: GitHub Projects or Issues を利用し、各ステップをカード化。PR テンプレートに「テスト実行結果」「README/CONTRIBUTING 更新有無」を必須項目として追加。

## セルフレビュー結果 (2025-02-14)
- **計画の妥当性**: Step 1〜7 の内容が目的（LSP型の二重管理解消、ワークスペース境界再構築、抽象化整備、ドキュメント更新）を網羅しており、目的達成に向けた道筋として妥当。
- **レガシー/冗長性**: 新規追記分に実装手順の重複や不整合は見当たらない。document層・analysis層の再編に向けた抽象化方針も一貫している。
- **ドキュメント最新性**: README.md/CONTRIBUTING.md/CLAUDE.md は現状のリファクタリング方針を反映しておらず、Step 6 のタスクとして改版が必要。

### 追加作業計画
- [ ] Step 6 実行時に README.md/CONTRIBUTING.md/CLAUDE.md を計画内容に合わせて更新する具体手順を Issue 化。
- [ ] Step 4〜5 実装開始前に `DocumentRepository`/`AnalysisRuntime` の API 草案を共有ドキュメントにまとめ、レビューを受ける。
- [ ] 週次でセルフレビューを行い、本ファイルの進捗・気付きを更新する運用を徹底する。
