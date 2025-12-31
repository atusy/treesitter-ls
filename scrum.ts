// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Expand LSP bridge to support most language server features indirectly through bridging (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, references, rename, codeAction, formatting",
      },
      {
        metric: "Modular architecture",
        target: "Bridge module split into per-feature files for maintainability",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-118 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    {
      id: "PBI-111",
      story: {
        role: "Rustacean editing Markdown",
        capability: "get completion suggestions for Rust code blocks via bridge",
        benefit: "I can use familiar completion features without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion: "src/lsp/bridge/completion.rs exists with CompletionWithNotifications type",
          verification: "grep 'CompletionWithNotifications' src/lsp/bridge/completion.rs returns matches",
        },
        {
          criterion: "LanguageServerConnection has completion_with_notifications method",
          verification: "cargo test completion_with_notifications --lib passes (unit test in connection.rs)",
        },
        {
          criterion: "textDocument/completion requests in injection regions are bridged to rust-analyzer",
          verification: "make test_nvim_file FILE=tests/test_lsp_completion.lua passes (E2E test)",
        },
        {
          criterion: "Completion results have textEdit ranges adjusted to host document positions",
          verification: "E2E test verifies completion textEdit range is in Markdown line numbers, not virtual document line numbers",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-112",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see function signature help for Rust code blocks via bridge",
        benefit:
          "I can see parameter hints while calling functions without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/signature_help.rs exists with SignatureHelpWithNotifications type",
          verification:
            "grep 'SignatureHelpWithNotifications' src/lsp/bridge/signature_help.rs returns matches",
        },
        {
          criterion:
            "textDocument/signatureHelp requests in injection regions are bridged",
          verification:
            "cargo test signature_help --lib passes (unit test in connection.rs)",
        },
        {
          criterion: "E2E test tests/test_lsp_signature_help.lua passes",
          verification:
            "make test_nvim_file FILE=tests/test_lsp_signature_help.lua passes",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-113",
      story: {
        role: "Rustacean editing Markdown",
        capability: "find all references for symbols in Rust code blocks via bridge",
        benefit:
          "I can navigate and understand code usage without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/references.rs exists with ReferencesWithNotifications type",
          verification:
            "grep 'ReferencesWithNotifications' src/lsp/bridge/references.rs returns matches",
        },
        {
          criterion:
            "LanguageServerConnection has references_with_notifications method",
          verification:
            "cargo test references --lib passes (unit test in connection.rs)",
        },
        {
          criterion:
            "textDocument/references requests in injection regions are bridged",
          verification: "make test_nvim_file FILE=tests/test_lsp_references.lua passes",
        },
        {
          criterion:
            "Reference locations have ranges adjusted to host document positions",
          verification:
            "E2E test verifies reference ranges are in Markdown line numbers",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-114",
      story: {
        role: "Rustacean editing Markdown",
        capability: "rename symbols in Rust code blocks via bridge",
        benefit:
          "I can refactor code consistently without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/rename.rs exists with RenameWithNotifications type",
          verification:
            "grep 'RenameWithNotifications' src/lsp/bridge/rename.rs returns matches",
        },
        {
          criterion:
            "LanguageServerConnection has rename_with_notifications method",
          verification:
            "cargo test rename --lib passes (unit test in connection.rs)",
        },
        {
          criterion:
            "textDocument/rename requests in injection regions are bridged",
          verification: "make test_nvim_file FILE=tests/test_lsp_rename.lua passes",
        },
        {
          criterion:
            "WorkspaceEdit TextEdit ranges adjusted to host document positions",
          verification:
            "E2E test verifies rename edit ranges are in Markdown line numbers",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-115",
      story: {
        role: "Rustacean editing Markdown",
        capability: "get code actions for Rust code blocks via bridge",
        benefit:
          "I can use quick fixes and refactorings without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/code_action.rs exists with CodeActionWithNotifications type",
          verification:
            "grep 'CodeActionWithNotifications' src/lsp/bridge/code_action.rs returns matches",
        },
        {
          criterion:
            "LanguageServerConnection has code_action_with_notifications method",
          verification:
            "cargo test code_action --lib passes (unit test in connection.rs)",
        },
        {
          criterion:
            "textDocument/codeAction requests in injection regions are bridged",
          verification: "make test_nvim_file FILE=tests/test_lsp_code_action.lua passes",
        },
        {
          criterion:
            "CodeAction edit ranges and diagnostic ranges adjusted to host document positions",
          verification:
            "E2E test verifies code action edit ranges are in Markdown line numbers",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-116",
      story: {
        role: "Rustacean editing Markdown",
        capability: "format Rust code blocks via bridge",
        benefit:
          "I can keep my code blocks consistently formatted without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/formatting.rs exists with FormattingWithNotifications type",
          verification:
            "grep 'FormattingWithNotifications' src/lsp/bridge/formatting.rs returns matches",
        },
        {
          criterion:
            "LanguageServerConnection has formatting_with_notifications method",
          verification:
            "cargo test formatting --lib passes (unit test in connection.rs)",
        },
        {
          criterion:
            "textDocument/formatting requests format all injection regions",
          verification: "make test_nvim_file FILE=tests/test_lsp_formatting.lua passes",
        },
        {
          criterion:
            "TextEdit ranges adjusted to host document positions",
          verification:
            "E2E test verifies formatting edit ranges are in Markdown line numbers",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-117",
      story: {
        role: "Rustacean editing Markdown",
        capability:
          "get code actions from both the Rust code block AND the Markdown document",
        benefit:
          "I can access both language-specific and host document code actions in one place",
      },
      acceptance_criteria: [
        {
          criterion:
            "Code actions from injection region (child) are returned first",
          verification:
            "E2E test verifies bridged actions appear before treesitter-ls actions",
        },
        {
          criterion:
            "Code actions from host document (parent) are appended after child actions",
          verification:
            "E2E test verifies treesitter-ls Inspect token action appears after bridged actions",
        },
        {
          criterion:
            "E2E test verifies merged code actions show child actions before parent actions",
          verification:
            "make test_nvim_file FILE=tests/test_lsp_code_action.lua passes with ordering assertions",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-118",
      story: {
        role: "developer editing Lua files",
        capability: "see LSP Bridge features documented in the README",
        benefit:
          "I can learn how to configure and use bridged LSP features in injection regions",
      },
      acceptance_criteria: [
        {
          criterion:
            "README.md Features section lists LSP Bridge with brief description",
          verification:
            "grep 'LSP Bridge' README.md returns matches in Features section",
        },
        {
          criterion:
            "README.md has LSP Bridge section with supported features list",
          verification:
            "README.md contains Completion, Signature Help, Go to Definition, Hover, Find References, Rename, Code Actions, Formatting",
        },
        {
          criterion:
            "README.md has bridge configuration example with servers and languages",
          verification:
            "README.md contains JSON example showing bridge.servers and languages.*.bridge configuration",
        },
        {
          criterion: "README.md explains bridge filter semantics",
          verification:
            "README.md documents bridge: [languages], bridge: [], and bridge: null/omitted behaviors",
        },
        {
          criterion: "README.md has Neovim example with bridge configuration",
          verification:
            "README.md contains Lua example showing vim.lsp.config with bridge options",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-119",
      story: {
        role: "developer editing Lua files",
        capability:
          "configure bridge language servers using 'languageServers' at the root level of init_options",
        benefit:
          "I can use a simpler, flatter configuration schema that aligns with common LSP patterns",
      },
      acceptance_criteria: [
        {
          criterion:
            "TreeSitterSettings accepts 'languageServers' field as HashMap<String, BridgeServerConfig>",
          verification:
            "cargo test should_parse_language_servers_at_root passes (unit test in settings.rs)",
        },
        {
          criterion:
            "BridgeSettings struct is removed or deprecated in favor of direct languageServers field",
          verification:
            "grep 'pub struct BridgeSettings' src/config/settings.rs returns no matches OR struct has deprecation comment",
        },
        {
          criterion:
            "LSP implementation uses languageServers from WorkspaceSettings for bridge pool initialization",
          verification:
            "cargo test --lib passes with bridge pool using languageServers instead of bridge.servers",
        },
        {
          criterion:
            "README.md and docs/README.md updated to show new languageServers configuration schema",
          verification:
            "grep 'languageServers' README.md returns matches; grep 'bridge.servers' README.md returns no matches in config examples",
        },
        {
          criterion:
            "E2E tests pass with the new configuration schema in minimal_init.lua",
          verification: "make test_nvim passes with languageServers configuration",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-120",
      story: {
        role: "Rustacean editing Markdown",
        capability:
          "configure per-language bridge filters using a map structure with enabled flag",
        benefit:
          "I can explicitly enable/disable bridging for specific injection languages with room for future per-language options",
      },
      acceptance_criteria: [
        {
          criterion:
            "LanguageConfig.bridge accepts map structure: { 'python': { 'enabled': true }, 'r': { 'enabled': false } }",
          verification:
            "cargo test should_parse_language_config_with_bridge_map passes (unit test in settings.rs)",
        },
        {
          criterion:
            "BridgeLanguageConfig struct exists with 'enabled' field",
          verification:
            "grep 'pub struct BridgeLanguageConfig' src/config/settings.rs returns matches",
        },
        {
          criterion:
            "LanguageSettings.bridge uses HashMap<String, BridgeLanguageConfig> instead of Vec<String>",
          verification:
            "grep 'bridge: Option<HashMap<String, BridgeLanguageConfig>>' src/config/settings.rs returns matches",
        },
        {
          criterion:
            "is_language_bridgeable method checks enabled field in the map",
          verification:
            "cargo test test_bridge_filter_map_enabled passes with new logic",
        },
        {
          criterion:
            "README.md and docs/README.md updated to show new bridge map configuration schema",
          verification:
            "grep 'enabled' README.md returns matches in bridge configuration examples",
        },
        {
          criterion:
            "E2E tests pass with the new bridge map configuration in minimal_init.lua",
          verification: "make test_nvim passes with bridge map configuration",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 97,
    pbi_id: "PBI-120",
    goal: "Change per-language bridge filter from Vec<String> to HashMap<String, BridgeLanguageConfig> with enabled flag",
    status: "planning",
    subtasks: [
      {
        test: "BridgeLanguageConfig struct exists with 'enabled: bool' field and derives Deserialize/Serialize",
        implementation:
          "Add pub struct BridgeLanguageConfig { pub enabled: bool } to settings.rs with serde derives",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "LanguageConfig.bridge parses as Option<HashMap<String, BridgeLanguageConfig>>",
        implementation:
          "Change LanguageConfig.bridge type from Option<Vec<String>> to Option<HashMap<String, BridgeLanguageConfig>>",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "LanguageSettings.bridge uses HashMap<String, BridgeLanguageConfig> type",
        implementation:
          "Change LanguageSettings.bridge type and update with_bridge constructor signature",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "is_language_bridgeable method checks enabled field in the map",
        implementation:
          "Update is_language_bridgeable to lookup language in map and check enabled field; None or missing key = not bridgeable",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Existing unit tests updated to use new bridge map syntax",
        implementation:
          "Update test_bridge_filter_* tests to use HashMap<String, BridgeLanguageConfig> instead of Vec<String>",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "README.md and docs/README.md show new bridge map configuration with enabled flag",
        implementation:
          "Update bridge configuration examples to show { 'python': { 'enabled': true } } syntax",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "E2E tests pass with bridge map configuration in minimal_init.lua",
        implementation:
          "Verify minimal_init.lua already uses correct syntax; run make test_nvim",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
    ],
  },

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 96,
      pbi_id: "PBI-119",
      goal: "Simplify bridge configuration by moving languageServers to root level of init_options",
      status: "done",
      subtasks: [],
    },
    {
      number: 95,
      pbi_id: "PBI-118",
      goal: "Update README with LSP Bridge documentation",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 96,
      improvements: [
        {
          action:
            "Schema simplification - moved languageServers from nested bridge.servers to root level for flatter configuration",
          timing: "immediate",
          status: "completed",
          outcome:
            "languageServers field now at root level of init_options; BridgeSettings wrapper removed; all E2E tests passing",
        },
      ],
    },
    {
      sprint: 95,
      improvements: [
        {
          action:
            "Documentation sprint - straightforward README update with bridge configuration examples",
          timing: "immediate",
          status: "completed",
          outcome:
            "README updated with LSP Bridge section including supported features, JSON and Lua configuration examples, and filter semantics",
        },
      ],
    },
  ],
};

// ============================================================
// Type Definitions (DO NOT MODIFY - request human review for schema changes)
// ============================================================

// PBI lifecycle: draft (idea) -> refining (gathering info) -> ready (can start) -> done
type PBIStatus = "draft" | "refining" | "ready" | "done";

// Sprint lifecycle
type SprintStatus =
  | "planning"
  | "in_progress"
  | "review"
  | "done"
  | "cancelled";

// TDD cycle: pending -> red (test written) -> green (impl done) -> refactoring -> completed
type SubtaskStatus = "pending" | "red" | "green" | "refactoring" | "completed";

// behavioral = changes observable behavior, structural = refactoring only
type SubtaskType = "behavioral" | "structural";

// Commits happen only after tests pass (green/refactoring), never on red
type CommitPhase = "green" | "refactoring";

// When to execute retrospective actions:
//   immediate: Apply within Retrospective (non-production code, single logical change)
//   sprint: Add as subtask to next sprint (process improvements)
//   product: Add as new PBI to Product Backlog (feature additions)
type ImprovementTiming = "immediate" | "sprint" | "product";

type ImprovementStatus = "active" | "completed" | "abandoned";

interface SuccessMetric {
  metric: string;
  target: string;
}

interface ProductGoal {
  statement: string;
  success_metrics: SuccessMetric[];
}

interface AcceptanceCriterion {
  criterion: string;
  verification: string;
}

interface UserStory {
  role: (typeof userStoryRoles)[number];
  capability: string;
  benefit: string;
}

interface PBI {
  id: string;
  story: UserStory;
  acceptance_criteria: AcceptanceCriterion[];
  status: PBIStatus;
}

interface Commit {
  hash: string;
  message: string;
  phase: CommitPhase;
}

interface Subtask {
  test: string;
  implementation: string;
  type: SubtaskType;
  status: SubtaskStatus;
  commits: Commit[];
  notes: string[];
}

interface Sprint {
  number: number;
  pbi_id: string;
  goal: string;
  status: SprintStatus;
  subtasks: Subtask[];
}

interface DoDCheck {
  name: string;
  run: string;
}

interface DefinitionOfDone {
  checks: DoDCheck[];
}

interface Improvement {
  action: string;
  timing: ImprovementTiming;
  status: ImprovementStatus;
  outcome: string | null;
}

interface Retrospective {
  sprint: number;
  improvements: Improvement[];
}

interface ScrumDashboard {
  product_goal: ProductGoal;
  product_backlog: PBI[];
  sprint: Sprint | null;
  definition_of_done: DefinitionOfDone;
  completed: Sprint[];
  retrospectives: Retrospective[];
}

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
