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

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113), PBI-155-161 (Sprint 124-130) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // Future: PBI-147 (hover wait), PBI-141/142/143 (async bridge methods)
    // ADR-0010: PBI-151 (118), PBI-150 (119), PBI-149 (120) | ADR-0011: PBI-152-155 (121-124)
    {
      id: "PBI-162",
      story: {
        role: "developer editing Lua files",
        capability: "receive empty responses from bridged language servers immediately after spawn but before initialized notification is sent",
        benefit: "requests don't fail or cause protocol errors while the connection is establishing, ensuring a robust editing experience during initialization window",
      },
      acceptance_criteria: [
        {
          criterion: "Each bridged language server connection instance tracks its own initialized flag as per-connection state",
          verification: "Unit test spawns 2 separate connections, verifies each has independent initialized_complete/initialized flag (one true, one false)",
        },
        {
          criterion: "Request methods (goto_definition, hover, completion, signature_help) return None immediately when initialized flag is false",
          verification: "Unit test with mock connection sets initialized=false, calls request methods, verifies all return None without sending LSP messages",
        },
        {
          criterion: "After initialized notification is sent, initialized flag becomes true and requests are forwarded normally",
          verification: "Unit test verifies flag transitions from false to true after spawn_and_initialize sends initialized notification, subsequent requests are forwarded to LSP server",
        },
        {
          criterion: "Both LanguageServerConnection (sync) and TokioAsyncBridgeConnection (async) implement initialization state tracking with consistent behavior",
          verification: "Tests for both connection types verify: (1) initialized flag exists, (2) guards work correctly in request methods, (3) flag lifecycle matches spawn->initialize->initialized flow",
        },
      ],
      status: "ready" as PBIStatus,
    },
  ],
  sprint: null,
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
      { name: "Documentation updated alongside implementation", run: "git diff --name-only | grep -E '(README|docs/|adr/)' || echo 'No docs updated - verify if needed'" },
      { name: "ADR verification for architectural changes", run: "git diff --name-only | grep -E 'adr/' || echo 'No ADR updated - verify if architectural change'" },
    ],
  },
  // Historical sprints (recent 2) | Sprint 1-129: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 130, pbi_id: "PBI-161", goal: "Update ADR-0010 and ADR-0011 to match implementation", status: "done", subtasks: [] },
    { number: 129, pbi_id: "PBI-160", goal: "Extract wildcard key to named constant for maintainability", status: "done", subtasks: [] },
  ],
  // Retrospectives (recent 2)
  retrospectives: [
    { sprint: 130, improvements: [
      { action: "Update documentation alongside implementation, not as separate PBI - add to Definition of Done", timing: "immediate", status: "completed", outcome: "Added documentation update check to Definition of Done" },
      { action: "Add ADR verification to Definition of Done to ensure architectural decisions are documented", timing: "immediate", status: "completed", outcome: "Added ADR verification check to Definition of Done" },
    ] },
    { sprint: 129, improvements: [
      { action: "Consider creating dedicated wildcard module for related constants to improve organization", timing: "product", status: "active", outcome: null },
      { action: "Add similar named constants for other magic strings in codebase to prevent typos", timing: "product", status: "active", outcome: null },
      { action: "Document structural refactoring pattern: pub(crate) visibility follows YAGNI principle when no external usage exists", timing: "immediate", status: "active", outcome: null },
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
