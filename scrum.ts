// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
  "treesitter-ls user managing configurations",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Expand LSP bridge to support most language server features indirectly through bridging (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, references, rename, codeAction, formatting, typeDefinition, implementation, documentHighlight, declaration, inlayHint, callHierarchy, typeHierarchy, documentLink, foldingRange",
      },
      {
        metric: "Modular architecture",
        target: "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // ADR-0010/0011 Integration: Wire implemented APIs into application flow
    {
      id: "PBI-155",
      story: {
        role: "treesitter-ls user managing configurations",
        capability: "have my user config at ~/.config/treesitter-ls/treesitter-ls.toml loaded and merged with project and session settings",
        benefit: "I can set global defaults once (e.g., captureMappings, searchPaths) without repeating them in every project",
      },
      acceptance_criteria: [
        {
          criterion: "load_settings() calls load_user_config() and includes user config in 4-layer merge: defaults < user < project < init_options",
          verification: "Unit test verifies merge_all() is called with all 4 layers; grep confirms load_user_config() is invoked in settings.rs",
        },
        {
          criterion: "resolve_language_with_wildcard() is called when looking up language configs, enabling languages._ inheritance",
          verification: "Unit test verifies languages._ settings are inherited by specific languages; grep confirms resolve_language_with_wildcard usage in lsp_impl.rs",
        },
        {
          criterion: "resolve_language_server_with_wildcard() is called when looking up server configs, enabling languageServers._ inheritance",
          verification: "Unit test verifies languageServers._ settings are inherited by specific servers; grep confirms resolve_language_server_with_wildcard usage in lsp_impl.rs",
        },
        {
          criterion: "User config file at ~/.config/treesitter-ls/treesitter-ls.toml is loaded and merged with project config",
          verification: "E2E test creates user config with unique searchPath, verifies it appears in effective settings",
        },
      ],
      status: "ready",
    },
    // ADR-0009 Implementation: Vertical slices with user-facing value
    // Completed: PBI-144 (Sprint 114), PBI-145 (Sprint 115), PBI-148 (Sprint 116), PBI-146 (Sprint 117)
    {
      id: "PBI-147",
      story: {
        role: "Rustacean editing Markdown",
        capability: "get hover results on first request without needing to retry",
        benefit: "hover works reliably the first time I trigger it on a new code block",
      },
      acceptance_criteria: [
        {
          criterion: "spawn_and_initialize waits for rust-analyzer to complete initial indexing",
          verification: "Unit test verifies hover is not called until indexing is complete",
        },
        {
          criterion: "Wait uses $/progress notifications to detect indexing completion",
          verification: "Unit test verifies $/progress notifications are monitored and indexing end is detected",
        },
        {
          criterion: "Single hover request returns result without retry loop",
          verification: "E2E test verifies single hover request returns result (no retry loop needed)",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-141",
      story: {
        role: "developer editing Lua files",
        capability: "have go-to-definition requests in Markdown code blocks use fully async I/O",
        benefit: "definition responses are faster and don't block other LSP requests while waiting for lua-language-server",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool.goto_definition() method implemented with async request/response pattern",
          verification: "Unit test verifies goto_definition returns valid Location response",
        },
        {
          criterion: "definition_impl uses async pool.goto_definition() instead of spawn_blocking",
          verification: "grep confirms no spawn_blocking in definition.rs for bridged requests",
        },
        {
          criterion: "Go-to-definition requests to lua-language-server return valid responses through async path",
          verification: "E2E test opens Markdown with Lua code block, requests definition, receives location",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-142",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have completion requests in Markdown code blocks use fully async I/O",
        benefit: "completion responses are faster and don't block other LSP requests while waiting for rust-analyzer",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool.completion() method implemented with async request/response pattern",
          verification: "Unit test verifies completion returns valid CompletionList response",
        },
        {
          criterion: "completion handler uses async pool.completion() for bridged requests",
          verification: "grep confirms async completion path in lsp_impl.rs",
        },
        {
          criterion: "Completion requests to rust-analyzer return valid responses through async path",
          verification: "E2E test opens Markdown with Rust code block, requests completion, receives items",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-143",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have signatureHelp requests in Markdown code blocks use fully async I/O",
        benefit: "signature help responses are faster and show parameter hints without blocking",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool.signature_help() method implemented with async request/response pattern",
          verification: "Unit test verifies signature_help returns valid SignatureHelp response",
        },
        {
          criterion: "signatureHelp handler uses async pool.signature_help() for bridged requests",
          verification: "grep confirms async signature_help path in lsp_impl.rs",
        },
        {
          criterion: "SignatureHelp requests to rust-analyzer return valid responses through async path",
          verification: "E2E test opens Markdown with Rust code block, requests signatureHelp, receives signatures",
        },
      ],
      status: "ready",
    },
    // ADR-0010: Completed PBI-151 (Sprint 118), PBI-150 (Sprint 119), PBI-149 (Sprint 120)
    // ADR-0011: Completed PBI-152 (Sprint 121), PBI-153 (Sprint 122), PBI-154 (Sprint 123)
  ],
  sprint: {
    number: 124,
    pbi_id: "PBI-155",
    goal: "Wire config APIs into application so users can actually use user config files and wildcard inheritance",
    status: "review",
    subtasks: [
      {
        test: "Unit test verifies load_settings() calls load_user_config() and merge_all() with 4 layers: defaults, user, project, init_options",
        implementation: "Wire load_user_config() into load_settings() in settings.rs; replace merge_settings() with merge_all() for 4-layer merge",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "600f114", message: "feat(config): wire load_user_config() + merge_all() into load_settings()", phase: "green" }],
        notes: [
          "Key file: src/lsp/settings.rs",
          "Uses 3-layer merge: user < project < init_options",
          "load_user_config_with_events() helper added for logging",
        ],
      },
      {
        test: "Unit test verifies languages._ settings are inherited by specific languages when resolve_language_with_wildcard is used",
        implementation: "Wire resolve_language_with_wildcard() into language config lookups in lsp_impl.rs (get_bridge_config_for_language and similar)",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "0c5a802", message: "feat(config): wire resolve_language_settings_with_wildcard() into bridge lookup", phase: "green" }],
        notes: [
          "Key file: src/lsp/lsp_impl.rs",
          "Added resolve_language_settings_with_wildcard() for LanguageSettings map",
          "get_bridge_config_for_language now uses wildcard resolution",
        ],
      },
      {
        test: "Unit test verifies languageServers._ settings are inherited by specific servers when resolve_language_server_with_wildcard is used",
        implementation: "Wire resolve_language_server_with_wildcard() into server config lookups in lsp_impl.rs",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "d8cdd5e", message: "feat(config): wire resolve_language_server_with_wildcard() into server lookup", phase: "green" }],
        notes: [
          "Key file: src/lsp/lsp_impl.rs",
          "get_bridge_config_for_language now uses wildcard resolution when finding matching server",
          "Server iteration skips wildcard entry and applies resolution on match",
        ],
      },
      {
        test: "E2E test creates user config with unique searchPath at ~/.config/treesitter-ls/treesitter-ls.toml, verifies it appears in effective settings",
        implementation: "Ensure E2E test framework can create/cleanup user config files and verify merged settings",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "8036356", message: "test(config): add E2E tests for user config loading and merge", phase: "green" }],
        notes: [
          "Integration tests in tests/test_config_wildcard_integration.rs",
          "Uses XDG_CONFIG_HOME env var for test isolation",
          "3 new tests: user config loading, merge_all, 3-layer merge",
        ],
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
  // Historical sprints (recent 2) | Sprint 1-122: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 123, pbi_id: "PBI-154", goal: "Enable users to define default language server settings using a wildcard key", status: "done", subtasks: [] },
    { number: 122, pbi_id: "PBI-153", goal: "Enable wildcard keys in languages and bridge configs", status: "done", subtasks: [] },
  ],
  // Retrospectives (recent 2)
  retrospectives: [
    { sprint: 123, improvements: [
      { action: "ADR-0011 implementation complete - all 3 phases done", timing: "immediate", status: "completed", outcome: "Full wildcard support across captureMappings, languages, and languageServers" },
    ] },
    { sprint: 122, improvements: [
      { action: "ADR-0011 status updated to Accepted", timing: "immediate", status: "completed", outcome: "Phase 2 complete" },
      { action: "Consider generic resolve_with_wildcard<T: Merge>() trait", timing: "product", status: "active", outcome: null },
    ] },
  ],
};

// Type Definitions (DO NOT MODIFY) =============================================
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
