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
      status: "done",
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
  // Sprint 1-59 details: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 63,
      pbi_id: "PBI-082",
      goal: "Establish injection region tracking infrastructure with byte ranges and cached token association",
      status: "done",
      subtasks: [],
    },
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
  ],

  // Sprint 62 retro: Tidy First worked well. Sprint item -> Sprint 63 subtask 1.
  // Product items: memory optimization (previous_text), update perf metrics (4.6ms vs 20ms)
  // Sprint 63 retro: Clean architecture, retro follow-through, all ACs verified with unit tests.
  retrospectives: [
    {
      sprint: 63,
      improvements: [
        {
          action: "Add module-level doc comment to semantic_cache.rs explaining injection cache architecture",
          timing: "immediate",
          status: "completed",
          outcome: "Added comprehensive module docs with ASCII architecture diagram",
        },
        {
          action: "Wire InjectionMap/InjectionTokenCache into lsp_impl.rs for invalidation",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
        {
          action: "Create benchmark script for injection-heavy docs before PBI-084",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
        {
          action: "Add contains_byte() helper to CacheableInjectionRegion for simpler byte-range lookup",
          timing: "product",
          status: "active",
          outcome: null,
        },
      ],
    },
  ],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
