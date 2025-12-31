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

  // Completed PBIs: PBI-001 through PBI-119 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    {
      id: "PBI-120",
      story: {
        role: "developer editing Lua files",
        capability:
          "configure language parser and queries using a unified schema with 'parser' field and 'queries' array",
        benefit:
          "I have a cleaner, more flexible configuration that supports custom query kinds and explicit ordering",
      },
      acceptance_criteria: [
        {
          criterion:
            "New schema accepts 'parser' field as alias for 'library' (both work during deprecation period)",
          verification:
            "Unit test: LanguageConfig deserializes both {parser: '/path'} and {library: '/path'} to same internal representation",
        },
        {
          criterion:
            "New schema accepts 'queries' array with {path, kind?} objects",
          verification:
            "Unit test: LanguageConfig deserializes queries: [{path: '/p.scm'}] and infers kind from filename",
        },
        {
          criterion:
            "Query kind is inferred from filename when not specified (e.g., 'highlights.scm' -> 'highlights')",
          verification:
            "Unit test: infer_query_kind('highlights.scm') returns 'highlights', 'injections.scm' returns 'injections'",
        },
        {
          criterion:
            "Explicit 'kind' field in query object overrides filename inference",
          verification:
            "Unit test: {path: '/custom.scm', kind: 'injections'} is treated as injections query",
        },
        {
          criterion:
            "Old schema fields (library, highlights, locals, injections) still work but emit deprecation warning to LSP log",
          verification:
            "Unit test: Deserializing old schema succeeds; integration test: deprecation warning appears in LSP log",
        },
        {
          criterion:
            "Internal LanguageSettings uses unified queries representation regardless of input schema format",
          verification:
            "Unit test: Both old and new schema formats convert to identical LanguageSettings with queries Vec<QueryConfig>",
        },
        {
          criterion: "Documentation updated with new schema and migration guide",
          verification:
            "README shows new schema as primary, old schema as deprecated with migration examples",
        },
      ],
      status: "ready",
    },
  ],

  sprint: null,

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
      number: 98,
      pbi_id: "PBI-120",
      goal: "Add queries array with kind inference from filename",
      status: "done",
      subtasks: [],
    },
    {
      number: 97,
      pbi_id: "PBI-120",
      goal: "Add parser field as alias for library with deprecation warning",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 98,
      improvements: [
        {
          action:
            "Clean TDD implementation with ~20 new tests - effective_queries() pattern elegantly handles both old and new config formats, consistent with effective_parser() from Sprint 97",
          timing: "immediate",
          status: "completed",
          outcome:
            "QueryConfig type with infer_query_kind() using file stem provides simple, predictable API; unified representation simplifies all downstream query consumption code",
        },
        {
          action:
            "Extended existing deprecation warning pattern seamlessly - uses_deprecated_query_fields() and log_deprecation_warnings() extension followed established pattern from Sprint 97",
          timing: "immediate",
          status: "completed",
          outcome:
            "Deprecation detection for highlights/injections/locals fields integrated without any architectural changes; pattern proves reusable across deprecation scenarios",
        },
        {
          action:
            "Consider splitting multi-phase PBIs upfront - PBI-120 spans 3 sprints (97-99); in future, might define Phase 1/2/3 as separate PBIs at refinement time",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
        {
          action:
            "E2E test naming issue (treesitter_ls vs treesitter-ls) still needs fixing - carried forward from Sprint 96/97",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
      ],
    },
    {
      sprint: 97,
      improvements: [
        {
          action:
            "Clean TDD implementation - 7 new tests covering all edge cases for parser field alias with effective_parser() pattern cleanly abstracting field preference logic",
          timing: "immediate",
          status: "completed",
          outcome:
            "Backwards compatible parser/library dual-field approach allows detecting which field was used for deprecation warnings; log_deprecation_warnings() placed at settings loading integration point",
        },
        {
          action:
            "Deprecation warning pattern (uses_deprecated_*, log_deprecation_warnings) is reusable for future deprecations",
          timing: "immediate",
          status: "completed",
          outcome:
            "Pattern established: separate fields rather than serde alias enables detection of deprecated usage while maintaining full backwards compatibility",
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
