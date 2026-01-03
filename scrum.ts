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

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113), PBI-155-160 (Sprint 124-129) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    {
      id: "PBI-161",
      story: {
        role: "treesitter-ls user managing configurations",
        capability: "update ADR-0010 and ADR-0011 to match actual implementation behavior",
        benefit: "documentation accurately reflects how the system works and prevents user confusion",
      },
      acceptance_criteria: [
        {
          criterion: "ADR-0010 lines 48-65: Update type inference rules and examples table to show exact filename matching only (highlights.scm, locals.scm, injections.scm), removing pattern-based examples like *highlights*.scm",
          verification: "Table at lines 58-65 shows only exact filename matches; rule at line 50 states 'If the filename is exactly highlights.scm, locals.scm, or injections.scm'",
        },
        {
          criterion: "ADR-0010 line 89: Update legacy field merge behavior from 'queries entries are processed first, then legacy fields append' to 'when queries field is present, legacy fields are ignored entirely'",
          verification: "Line 89 states queries field takes complete precedence, with reference to coordinator.rs:388-395",
        },
        {
          criterion: "ADR-0011 lines 58-59: Remove (future) tags from languages._, languages.{lang}.bridge._, and languageServers._ as these are implemented in Sprints 122-123",
          verification: "Table shows all three wildcard patterns without (future) annotations",
        },
      ],
      status: "ready",
    },
    // Future: PBI-147 (hover wait), PBI-141/142/143 (async bridge methods)
    // ADR-0010: PBI-151 (118), PBI-150 (119), PBI-149 (120) | ADR-0011: PBI-152-155 (121-124)
  ],
  sprint: null,
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },
  // Historical sprints (recent 2) | Sprint 1-128: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 129, pbi_id: "PBI-160", goal: "Extract wildcard key to named constant for maintainability", status: "done", subtasks: [] },
    { number: 128, pbi_id: "PBI-159", goal: "Add comprehensive tests for coordinator unified query loading", status: "done", subtasks: [] },
  ],
  // Retrospectives (recent 2)
  retrospectives: [
    { sprint: 129, improvements: [
      { action: "Consider creating dedicated wildcard module for related constants to improve organization", timing: "product", status: "active", outcome: null },
      { action: "Add similar named constants for other magic strings in codebase to prevent typos", timing: "product", status: "active", outcome: null },
      { action: "Document structural refactoring pattern: pub(crate) visibility follows YAGNI principle when no external usage exists", timing: "immediate", status: "active", outcome: null },
    ] },
    { sprint: 128, improvements: [
      { action: "Add error path tests for invalid query files to verify graceful failure handling", timing: "sprint", status: "active", outcome: null },
      { action: "Consider property-based testing for query loading edge cases to improve test coverage", timing: "product", status: "active", outcome: null },
      { action: "Document test helper pattern: register_language_for_test enables clean, realistic test setup", timing: "immediate", status: "active", outcome: null },
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
