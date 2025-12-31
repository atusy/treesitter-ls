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

  // Completed PBIs: PBI-001 through PBI-126 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  // PBI-120-126: Done - bridge features, text_document/ directory restructure, declaration
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
          verification: "LanguageServerConnection has declaration_with_notifications method that sends textDocument/declaration request",
        },
        {
          criterion: "E2E test verifies declaration works in injection regions",
          verification: "test_lsp_declaration.lua passes with rust-analyzer in Markdown code block",
        },
      ],
      status: "done",
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
      status: "done",
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
      status: "done",
    },
    {
      id: "PBI-129",
      story: {
        role: "Rustacean editing Markdown",
        capability: "explore type hierarchy (supertypes and subtypes) for types in embedded Rust code blocks",
        benefit: "I can understand trait implementations and inheritance relationships while documenting code",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/prepareTypeHierarchy bridge implemented",
          verification: "src/lsp/bridge/text_document/type_hierarchy.rs exists with PrepareTypeHierarchyWithNotifications type",
        },
        {
          criterion: "typeHierarchy/supertypes bridge implemented",
          verification: "SupertypesWithNotifications type handles typeHierarchy/supertypes requests",
        },
        {
          criterion: "typeHierarchy/subtypes bridge implemented",
          verification: "SubtypesWithNotifications type handles typeHierarchy/subtypes requests",
        },
        {
          criterion: "Type hierarchy positions correctly translated between injection and host coordinates",
          verification: "Type hierarchy items have correct locations within Markdown code blocks",
        },
        {
          criterion: "E2E test verifies type hierarchy works in injection regions",
          verification: "test_lsp_type_hierarchy.lua passes showing supertypes/subtypes for Rust type",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-130",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see and follow hyperlinks in embedded Rust code blocks",
        benefit: "I can navigate to URLs and external resources referenced in code comments or doc strings",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/documentLink bridge implemented",
          verification: "src/lsp/bridge/text_document/document_link.rs exists with DocumentLinkWithNotifications type",
        },
        {
          criterion: "DocumentLink request forwarded to bridged language server",
          verification: "LanguageServerConnection has document_link_with_notifications method",
        },
        {
          criterion: "DocumentLink positions correctly translated between injection and host coordinates",
          verification: "Links appear at correct positions within Markdown code blocks",
        },
        {
          criterion: "E2E test verifies document links work in injection regions",
          verification: "test_lsp_document_link.lua passes showing links in Rust code block",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-131",
      story: {
        role: "Rustacean editing Markdown",
        capability: "fold code regions in embedded Rust code blocks",
        benefit: "I can collapse function bodies and blocks to focus on code structure while documenting",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/foldingRange bridge implemented",
          verification: "src/lsp/bridge/text_document/folding_range.rs exists with FoldingRangeWithNotifications type",
        },
        {
          criterion: "FoldingRange request forwarded to bridged language server",
          verification: "LanguageServerConnection has folding_range_with_notifications method",
        },
        {
          criterion: "FoldingRange positions correctly translated between injection and host coordinates",
          verification: "Folding ranges have correct line numbers within Markdown code blocks",
        },
        {
          criterion: "E2E test verifies folding ranges work in injection regions",
          verification: "test_lsp_folding_range.lua passes showing foldable regions in Rust code block",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 108,
    pbi_id: "PBI-131",
    goal: "Add textDocument/foldingRange bridge support",
    status: "planning",
    subtasks: [],
  },

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-100: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 107, pbi_id: "PBI-130", goal: "Add textDocument/documentLink bridge support", status: "done", subtasks: [] },
    { number: 106, pbi_id: "PBI-129", goal: "Add typeHierarchy bridge (prepareTypeHierarchy, supertypes, subtypes)", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-99: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 107,
      improvements: [
        { action: "DocumentLink follows simple Vec<DocumentLink> response pattern similar to DocumentHighlight. Range field needs virtual-to-host translation", timing: "immediate", status: "completed", outcome: "Sprint 107 completed; simple single-method bridge" },
        { action: "rust-analyzer may not return document links for URLs in comments - E2E test verifies request completes without error", timing: "immediate", status: "completed", outcome: "E2E test handles empty results gracefully" },
      ],
    },
    {
      sprint: 106,
      improvements: [
        { action: "TypeHierarchy follows same pattern as CallHierarchy: 3 methods (prepare, supertypes, subtypes). TypeHierarchyItem has same fields as CallHierarchyItem", timing: "immediate", status: "completed", outcome: "Sprint 106 completed; reused callHierarchy pattern for fast implementation" },
        { action: "TypeHierarchyItem.data field has same opaque state issue as CallHierarchyItem - supertypes/subtypes may return empty results", timing: "immediate", status: "completed", outcome: "E2E test covers prepareTypeHierarchy only; documented limitation" },
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
