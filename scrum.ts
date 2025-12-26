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
  role: string;
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

// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Achieve high-performance semantic tokens delta: minimize latency for syntax highlighting updates during editing by leveraging caching, efficient delta algorithms, and Tree-sitter's incremental parsing.",
    success_metrics: [
      {
        metric: "Delta response (no change)",
        target: "<5ms via cache hit",
      },
      {
        metric: "Delta response (small edit)",
        target: "<20ms via incremental",
      },
      {
        metric: "Delta transfer size",
        target: "Reduced via suffix matching",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-081
  // For historical details: git log -- scrum.yaml, scrum.ts
  // Design reference: __ignored/semantic-token-performance.md
  // PBI-079 split into: PBI-082 (infrastructure) -> PBI-083 (invalidation) -> PBI-084 (perf)
  product_backlog: [
    {
      id: "PBI-082",
      story: {
        role: "developer editing multi-language documents",
        capability: "have injection regions tracked with byte ranges and cached tokens",
        benefit: "enable targeted cache invalidation for injection-level incremental updates",
      },
      acceptance_criteria: [
        {
          criterion: "InjectionRegion struct captures language, byte range, line range, and result_id",
          verification: "Unit test verifies InjectionRegion fields populated correctly from parse tree",
        },
        {
          criterion: "InjectionMap tracks all injection regions for a document URI",
          verification: "Unit test verifies InjectionMap stores/retrieves regions by URI",
        },
        {
          criterion: "InjectionMap integrates with existing SemanticTokenCache",
          verification: "Unit test verifies cached tokens associated with injection regions",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-083",
      story: {
        role: "developer editing multi-language documents",
        capability: "have only injection regions overlapping edits re-tokenized",
        benefit: "skip unnecessary re-parsing of unchanged code blocks",
      },
      acceptance_criteria: [
        {
          criterion: "Edit outside injection regions preserves all injection caches",
          verification: "Integration test: edit in host text skips injection re-parse",
        },
        {
          criterion: "Edit inside injection region invalidates only that region's cache",
          verification: "Integration test: edit in code block re-parses only that block",
        },
        {
          criterion: "Structural changes (add/remove code block) update InjectionMap",
          verification: "Test verifies new code block triggers fresh region tracking",
        },
      ],
      status: "draft",
    },
    {
      id: "PBI-084",
      story: {
        role: "developer editing documents with many code blocks",
        capability: "experience <50% tokenization time when editing outside injections",
        benefit: "faster highlighting in documentation-heavy files",
      },
      acceptance_criteria: [
        {
          criterion: "Benchmark infrastructure for injection-aware tokenization",
          verification: "Benchmark script comparing full vs incremental on 5+ injection doc",
        },
        {
          criterion: "Performance target met: <50% of full tokenization time",
          verification: "Benchmark results documented in performance log",
        },
      ],
      status: "draft",
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

  // Historical sprints (keep recent 3 for learning)
  // Sprint 1-58 details: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 62,
      pbi_id: "PBI-081",
      goal: "Wire incremental tokenization path: use compute_incremental_tokens() when UseIncremental selected",
      status: "done",
      subtasks: [],
    },
    {
      number: 61,
      pbi_id: "PBI-080",
      goal: "Enable incremental tokenization path using merge_tokens() for <20ms highlighting updates",
      status: "done",
      subtasks: [],
    },
    {
      number: 60,
      pbi_id: "PBI-078",
      goal: "Implement incremental tokenization infrastructure using Tree-sitter changed_ranges() API",
      status: "done",
      subtasks: [],
    },
  ],

  retrospectives: [
    {
      sprint: 62,
      improvements: [
        {
          action:
            "Tidy First applied well - previous_text structural change separated from behavioral wiring",
          timing: "immediate",
          status: "completed",
          outcome: "Clean separation made review easier and reduced risk",
        },
        {
          action:
            "Document the UseIncremental -> compute_incremental_tokens() wiring in code comments for maintainability",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
        {
          action:
            "Evaluate memory optimization: previous_text storage vs on-demand diff computation",
          timing: "product",
          status: "active",
          outcome: null,
        },
        {
          action:
            "Performance exceeded target by 4x (4.6ms vs 20ms target) - consider updating product goal metrics to reflect actual capability",
          timing: "product",
          status: "active",
          outcome: null,
        },
      ],
    },
    // Sprint 61: All items completed - wiring verified, tests added, perf tracked in Sprint 62
  ],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
