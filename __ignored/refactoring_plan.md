# リファクタリング計画（進行中）

## 変更の背景と目的
- `analysis` が設定スキーマ (`config::CaptureMappings` など) の具体型に依存し、ドメイン境界が崩壊している。設定変更が解析層へ直接波及する危険を排除する。
- `analysis` から `document` 内部実装（`InjectionPositionMapper` 等）への直接アクセスが常態化し、抽象化が破綻している。文書層の内部表現を公開せず機能を提供できる構造へ改める。
- LSP 層が設定ロード・マージ・リトライ処理を内包し、薄いアダプタの原則を逸脱している。I/O や設定解釈は専用サービスへ委譲し、LSP はプロトコル橋渡しに専念させる。
- `domain` が LSP 表現・ビジネスエンティティ・ユーティリティを一括収容する God モジュール化を起こしている。責務の分離と意図の明文化が必要。
- Workspace まわりが設定ロードから解析指示、状態保存まで抱え込む God Object となっている。ユースケース単位のサービスへ機能を分割しテスト容易性を確保する。
- `runtime` は実質的に言語資産のライフサイクルを管理しており、命名と責務が乖離している。名称・階層を整理し、読者に意図が伝わる構造へ改める。
- `WorkspaceDocuments` / `WorkspaceLanguages` / `DocumentHandle` 等の薄いラッパーが、価値を生まない間接層として残骸化している。抽象追加ではなく保守性向上を狙った整理が必要。
- Query predicate 処理を `text` モジュールへ移した結果、テキスト補助と解析ロジックの責務が混線している。解析/言語サービス側で統合し、役割の境界を復元する。

## 変更方針
- プロジェクトは未公開。互換性は一切考慮せず、理想的なアーキテクチャを最短距離で実現する。
- 既存コードへの忖度や暫定対応は排除し、再設計を前提に破壊的変更を容認する。
- モジュール境界・命名・依存方向を全面的に再検討し、責務の単純化と理解容易性を最優先する。

## 優先付き変更計画（各ステップの DoD 付き）
1. [x] **設定スキーマ依存の排除**  
   - 実行内容: 設定情報を扱う新たなドメイン型を定義し、`analysis` / `workspace` から `config::` 直参照を外す。既存コードが依存する API の差分を洗い出し、新型へ移行させる。  
   - DoD: 依存方向が `config` → 中間ドメイン → `analysis`/`workspace` へ一本化され、`make test format lint` が成功する。
2. [x] **ポジションマッピングの抽象化**  
   - 実行内容: 文書層が提供すべきマッピング API を明文化し、`DocumentView` または専用サービスを通じて解析層が利用できるようにする。既存の `InjectionPositionMapper` 直接参照箇所を置換する。  
   - DoD: `analysis` から `document` 内部構造体への直接依存が消え、全テストが成功する。
3. [x] **設定ロード責務の移譲**  
   - 実行内容: 設定ファイル探索・パース・マージ・リトライ処理をまとめた新コンポーネントを用意し、LSP 層は結果の引き渡しのみを行う。  
   - DoD: LSP コードからファイル I/O と `toml` 依存が排除され、`make test format lint` が成功する。
4. [x] **`domain` モジュールの分割**  
   - 実行内容: ドメイン内の型を目的別（例: 位置/範囲、セマンティックトークン、コードアクション等）に分類し、サブモジュールへ再配置する。公開 API を再点検し、最小限の再エクスポートに絞る。  
   - DoD: `domain` 直下が整理され、ビルドと lint がクリーンに通る。
5. [x] **Domain/LSP 変換境界の再設計**  
   - 実行内容: LSP プロトコル型との相互変換を専用モジュール（`lsp/protocol` 仮）へ移し、ドメイン型は純粋なビジネス表現のみを扱う。  
   - DoD: `analysis` と `lsp` から見たドメイン依存が抽象型に限定され、`make test format lint` が成功する。
6. [x] **Workspace の責務分割**  
   - 実行内容（詳細化）: 
     - 6-1: クエリ取得・キャプチャマッピング・言語ロードなどのランタイム操作を `language_ops` へ集約し、`Workspace` から runtime 直接呼び出しを削減する。
     - 6-2: 文書指向の操作（解析結果の保持、テキストアクセス、Semantic Tokens 更新）を `document_ops` へ拡充し、`Workspace` の状態操作を薄くする。
     - 6-3: root path 管理や設定ロード経路の公開 API を小さく分割し、設定イベントは既存 `settings` モジュール経由に統一する。
     - 6-4: 上記分割後、`Workspace` がサービス配線と状態参照のみを担当することを確認し、`make test format lint` を成功させる。
7. [ ] **Runtime/Language 命名と責務の是正**  
   - 実行内容:  
     - 7-1: `runtime` モジュール内で言語ライフサイクルを担う要素（`RuntimeCoordinator`, `QueryStore`, `ParserLoader` など）を棚卸しし、純粋な言語サービスとして切り出す新モジュール構成を決定。  
     - 7-2: 責務と名称が一致するよう主要型をリネーム（例: `RuntimeCoordinator` → `LanguageCoordinator`）し、呼び出し側を更新。  
     - 7-3: 新モジュールへ移した API を公開し直し、Workspace や analysis からの依存を再配線。  
   - DoD: 言語管理ロジックが新モジュールに移行し、元の `runtime` には実行環境固有の責務のみが残り、`make test format lint` が成功する。
8. [ ] **薄いラッパー抽象の整理**  
   - 実行内容: `WorkspaceDocuments` / `WorkspaceLanguages` / `DocumentHandle` の役割を再評価し、不要なら削除、必要なら責務を再定義する。周辺コードの呼び出し口を見直し、直接依存へ置換する。  
   - DoD: 無価値な間接層が消え、`make test format lint` が成功する。
9. [ ] **Query 処理の責務整理**  
   - 実行内容: Query predicate 処理を適切な場所（言語サービスまたは専用ユーティリティ）へ移し、`text` モジュールから除外する。関連テストを更新する。  
   - DoD: Query 処理が責務に沿った場所へ移動し、`make test format lint` が成功する。

## 進捗状況
- 2025-09-17: 変更ルールを復唱。
  - 1. 各作業前にルール復唱を記録する。
  - 2. 動作可能な最小単位で `git commit` を積む。
  - 3. `make test format lint` を全コミット前に必ず実行し解決する。
  - 4. README / CONTRIBUTING を即時更新する。
  - 5. 計画・気付きを本ファイルへ逐次反映する。
- 2025-09-17: **Step1 完了**。
  - `domain::settings` を新設し、設定・キャプチャマッピングをドメイン型として再定義。
  - `config` に双方向変換 (`TreeSitterSettings` ⇄ `WorkspaceSettings`) を追加。
  - `analysis` / `workspace` が `config` を直接参照しないよう書き換え、`RuntimeCoordinator` をドメイン設定対応に変更。
  - `make format`, `make lint`, `make test` を実行してクリーンを確認。
- 2025-09-17: **Step2 完了**。
  - `DocumentView::position_mapper` を追加し、文書側で注入レイヤー対応を完結。
  - `analysis::definition` / `analysis::selection` から `InjectionPositionMapper` への直接依存を排除。
  - `make format`, `make lint`, `make test` を再実行し、影響範囲全体での正常動作を確認。
- 2025-09-17: **Step3 完了**。
  - `workspace::settings` を新設し、ファイル読込・JSON パース・マージを一か所へ集約。
  - LSP 層から `std::fs` / `toml` / `serde_json::from_value` 依存を排除し、イベントベースでログを受け取る構成へ変更。
  - `make format`, `make lint`, `make test` を実行し、変更後もクリーンを確認。
- 2025-09-17: **Step4 完了**。
  - `domain` を `position` / `selection` / `semantic` / `location` / `workspace_edit` / `code_action` の各サブモジュールへ分割し、責務を明確化。
  - 既存の API は `domain::` 直下で再エクスポートし、呼び出し側の変更を最小限に抑制。
  - `make format`, `make lint`, `make test` を実行し、構造変更後もクリーンを確認。
- 2025-09-17: **Step5 完了**。
  - `lsp/protocol` を新設し、LSP ↔ Domain 変換ロジックを集約。`TreeSitterLs` からファイルローカルな変換関数を削除。
  - 変換イベントは `protocol` 経由に統一し、LSP 層の責務をリクエスト制御に集中させた。
  - `make format`, `make lint`, `make test` を実行し、変更後もクリーンを確認。
- 2025-09-17: **Step6 着手**。
  - `workspace::document_ops` を追加し、`Workspace::parse_document` / `language_for_document` の主要ロジックをサービス関数へ退避。
  - Workspace 本体はサービス呼び出しの配線に集中させる。残りの責務分解は後続タスクで進める計画。
  - さらにドキュメント関連の補助処理（テキスト取得、トークン更新、削除）も `document_ops` に移し、泥臭い状態操作を一か所へまとめた。
  - 言語周辺処理（クエリ取得、キャプチャマッピング、パーサ管理、言語ロード）を `language_ops` に委譲し、`Workspace` が runtime へ直接触れないよう整理。
  - root path 管理用の `state` モジュールを導入し、`Workspace` が Mutex の詳細を意識せずに済むようにした。
  - `README.md` / `CONTRIBUTING.md` の `LanguageCoordinator` への名称変更を反映。
- 2025-09-17: **Step8 着手**。
  - `WorkspaceDocuments` を廃止し、`DocumentStore` を直接利用する構造へ変更。関連ロジックは `document_ops` に委譲。
  - `Workspace` は `DocumentHandle` を再エクスポートしつつ、薄い配線の役割に徹する。
- 2025-09-17: **Step7 着手**。
  - `language` モジュールを新設し、従来 `runtime` にあった言語ライフサイクル系の実装を移設・リネーム（`LanguageCoordinator` など）。
  - 既存の依存箇所（Workspace, LSP, tests, docs）を新 API へ置き換え。
  - `Workspace` が保持する public API はサービス呼び出しと状態ゲッターのみになったことを確認。`make format`, `make lint`, `make test` にてクリーンを確認。

## 変更中に得た気付き
- ドメイン側の設定構造は既存 `TreeSitterSettings` とほぼ同型のため、双方向 `From` 実装で十分に橋渡しできる。以降のステップでも変換ロジックを再利用する。
- `DocumentView` にマッピング生成を持たせれば、解析層の追加ロジック変更時も文書側で集中管理できる（テスト対象を絞りやすい）。
- 設定ロードの責務を専用モジュールへ移すと、ログの粒度も整理できるので LSP 層は単純なイベント転送に徹せる。
- ドメイン型を目的別に分割すると、後続のアダプタ実装が必要な型だけをインポートでき、依存把握が容易になった。
- 変換ロジックを `lsp/protocol` に集約したことで、`TreeSitterLs` は protocol API だけを呼べばよくなり、LSP 層の責務を観察しやすくなった。
- パース処理を `workspace::document_ops` に切り出すと、Workspace の public API を保ったまま内部実装を段階的に差し替えられると分かった。
- 言語系コンポーネントを新しい `language` モジュールに移すことで、`runtime` は設定ストア専用に整理され、依存関係の追跡が容易になった。
- 作業中に得た洞察・設計判断・トレードオフを随時箇条書きで記録する。

## 変更ルール
1. 各作業開始前に本ルールを復唱し、作業ログへ残す。
2. 動作可能な最小単位で `git commit` を作成する。破壊的変更でも区切りを細かく設定する。
3. すべての `git commit` 前に `make test format lint` を実行し、失敗を解消してからコミットする。
4. README.md / CONTRIBUTING.md など、影響を受けるドキュメントを即時更新する。
5. 進捗・気付き・方針変更はこの`__ignored/refactoring_plan.md`へ随時反映し、計画と実態を同期させる。
