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
          "Support completion, signatureHelp, references, rename, codeAction, formatting, typeDefinition, implementation, documentHighlight, declaration, inlayHint, callHierarchy",
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

  // Completed PBIs: PBI-001 through PBI-125 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  // PBI-120-125: Done - bridge features and text_document/ directory restructure
  product_backlog: [
    {
      id: "PBI-125",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have the bridge module organized with a text_document/ subdirectory matching the lsp_impl structure",
        benefit: "the codebase remains consistent and maintainable as more textDocument/* bridge features are added",
      },
      acceptance_criteria: [
        {
          criterion: "Bridge text_document features moved to src/lsp/bridge/text_document/ subdirectory",
          verification: "Files completion.rs, hover.rs, signature_help.rs, definition.rs, type_definition.rs, implementation.rs, references.rs, rename.rs, code_action.rs, formatting.rs, document_highlight.rs exist under src/lsp/bridge/text_document/",
        },
        {
          criterion: "Non-text_document bridge files remain at src/lsp/bridge/ level",
          verification: "Files pool.rs, connection.rs, cleanup.rs, workspace.rs remain at src/lsp/bridge/",
        },
        {
          criterion: "Module structure updated with text_document submodule",
          verification: "src/lsp/bridge.rs declares mod text_document and re-exports appropriately",
        },
        {
          criterion: "All existing tests pass without modification",
          verification: "make test && make test_nvim passes",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-126",
      story: {
        role: "Rustacean editing Markdown",
        capability: "navigate to the declaration of a symbol in an embedded Rust code block",
        benefit: "I can find forward declarations and interface definitions even when editing documentation",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/declaration bridge implemented",
          verification: "src/lsp/bridge/text_document/declaration.rs exists with DeclarationWithNotifications type",
        },
        {
          criterion: "Declaration request forwarded to bridged language server",
          verification: "LanguageServerConnection has goto_declaration method that sends textDocument/declaration request",
        },
        {
          criterion: "E2E test verifies declaration works in injection regions",
          verification: "test_lsp_declaration.lua passes with rust-analyzer in Markdown code block",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-127",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see inlay hints for types and parameter names in embedded Rust code blocks",
        benefit: "I can understand inferred types and parameter names without leaving the documentation context",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/inlayHint bridge implemented",
          verification: "src/lsp/bridge/text_document/inlay_hint.rs exists with InlayHintWithNotifications type",
        },
        {
          criterion: "InlayHint request forwarded to bridged language server",
          verification: "LanguageServerConnection has inlay_hint method that sends textDocument/inlayHint request",
        },
        {
          criterion: "Inlay hint positions correctly translated between injection and host coordinates",
          verification: "Hints appear at correct positions within Markdown code blocks",
        },
        {
          criterion: "E2E test verifies inlay hints work in injection regions",
          verification: "test_lsp_inlay_hint.lua passes showing type hints in Rust code block",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-128",
      story: {
        role: "Rustacean editing Markdown",
        capability: "explore call hierarchy (incoming and outgoing calls) for functions in embedded Rust code blocks",
        benefit: "I can understand function relationships and call patterns while documenting code",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/prepareCallHierarchy bridge implemented",
          verification: "src/lsp/bridge/text_document/call_hierarchy.rs exists with PrepareCallHierarchyWithNotifications type",
        },
        {
          criterion: "callHierarchy/incomingCalls bridge implemented",
          verification: "IncomingCallsWithNotifications type handles callHierarchy/incomingCalls requests",
        },
        {
          criterion: "callHierarchy/outgoingCalls bridge implemented",
          verification: "OutgoingCallsWithNotifications type handles callHierarchy/outgoingCalls requests",
        },
        {
          criterion: "Call hierarchy positions correctly translated between injection and host coordinates",
          verification: "Call hierarchy items have correct locations within Markdown code blocks",
        },
        {
          criterion: "E2E test verifies call hierarchy works in injection regions",
          verification: "test_lsp_call_hierarchy.lua passes showing incoming/outgoing calls for Rust function",
        },
      ],
      status: "ready",
    },
  ],

  sprint: null, // Sprint 102 (PBI-125) completed - bridge text_document directory restructure

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-99: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 102, pbi_id: "PBI-125", goal: "Restructure bridge directory with text_document/ subdirectory", status: "done", subtasks: [] },
    { number: 101, pbi_id: "PBI-124", goal: "Add textDocument/documentHighlight bridge support", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-98: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 100,
      improvements: [
        { action: "Copy-and-adapt pattern from Sprint 99 proved highly effective: implementation.rs copied from type_definition.rs with 3 string replacements", timing: "immediate", status: "completed", outcome: "Sprint 100 completed in fraction of Sprint 99 time due to established pattern" },
        { action: "Bridge feature velocity now predictable: ~15 min per new GotoDefinitionResponse-based method (definition, typeDefinition, implementation)", timing: "immediate", status: "completed", outcome: "documentHighlight (PBI-124) should follow same pattern with DocumentHighlight response type" },
      ],
    },
    {
      sprint: 99,
      improvements: [
        { action: "Established reusable bridge pattern: typeDefinition implemented by copying definition.rs with minimal changes", timing: "immediate", status: "completed", outcome: "New bridge features can follow copy-and-adapt pattern" },
        { action: "E2E tests for language servers with indexing require explicit wait; added vim.uv.sleep(2000)", timing: "immediate", status: "completed", outcome: "test_lsp_type_definition.lua passes reliably" },
        { action: "Consider code generation or macro for bridge methods - definition/typeDefinition/implementation share 95%+ identical code", timing: "product", status: "active", outcome: null },
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
