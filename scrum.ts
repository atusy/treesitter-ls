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

  // Completed PBIs: PBI-001 through PBI-080
  // For historical details: git log -- scrum.yaml, scrum.ts
  // Design reference: __ignored/semantic-token-performance.md
  product_backlog: [
    {
      id: "PBI-081",
      story: {
        role: "developer editing a document",
        capability:
          "have the incremental tokenization strategy decision actually used",
        benefit:
          "only changed regions are re-tokenized, reducing latency for small edits",
      },
      acceptance_criteria: [
        {
          criterion:
            "UseIncremental strategy triggers compute_incremental_tokens()",
          verification:
            "Unit test verifies incremental path is invoked when strategy is UseIncremental",
        },
        {
          criterion: "Unchanged regions preserve cached tokens",
          verification:
            "Integration test: edit on line N preserves tokens from lines outside changed range",
        },
        {
          criterion: "Sprint 61 retrospective improvements completed",
          verification:
            "Both 'sprint' timing improvements marked as completed with outcomes documented",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-079",
      story: {
        role: "developer editing a document with embedded languages (e.g., Markdown with code blocks)",
        capability:
          "have only affected injection regions re-tokenized when editing",
        benefit:
          "experience faster highlighting in multi-language documents where most injections remain unchanged",
      },
      acceptance_criteria: [
        {
          criterion:
            "Injection regions are tracked with their byte/line ranges and cached tokens",
          verification:
            "Unit test verifies InjectionMap correctly tracks injection regions from parse tree",
        },
        {
          criterion:
            "Only injections overlapping with changed ranges are re-tokenized",
          verification:
            "Integration test: edit in host document outside code blocks skips injection re-parse",
        },
        {
          criterion:
            "Injection structure changes (add/remove code block) invalidate relevant caches",
          verification:
            "Test verifies adding new code block triggers fresh tokenization for that block",
        },
        {
          criterion:
            "Performance improvement for documents with many unchanged injections",
          verification:
            "Benchmark shows reduced latency when editing outside injection regions",
        },
      ],
      status: "draft",
    },
  ],

  sprint: {
    number: 62,
    pbi_id: "PBI-081",
    goal: "Wire incremental tokenization path: use compute_incremental_tokens() when UseIncremental strategy selected",
    status: "review",
    subtasks: [
      // All subtasks completed
      {
        test: "Verify wire_incremental_path test exists and fails showing compute_incremental_tokens is not called",
        implementation: "Add test in lsp_impl.rs or integration test that verifies incremental path is invoked",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Sprint 61 retro: 'Verify incremental path fully wired'"],
      },
      {
        test: "Test passes after wiring compute_incremental_tokens() into semantic_tokens_full_delta",
        implementation: "When UseIncremental: decode cached tokens, call compute_incremental_tokens(), encode result",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Use decode_semantic_tokens() on cached tokens, encode_semantic_tokens() on result"],
      },
      {
        test: "Integration test: edit on line N preserves tokens from lines outside changed range",
        implementation: "Test that modifying line 5 of 10 preserves cached tokens from lines 0-4 and 6-9",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Sprint 61 retro: 'Add integration test verifying incremental path reduces work'"],
      },
      {
        test: "Document stores previous_text for line delta calculation",
        implementation: "Add previous_text field to Document, update in update_tree_and_text()",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["Added update_tree_and_text() for atomic update preserving previous state"],
      },
      {
        test: "Verify UseFull strategy still uses full tokenization path",
        implementation: "Ensure large changes (>30% or >10 ranges) bypass incremental path",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Regression tests added for no previous_tree and large structural changes"],
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

  // Historical sprints (keep recent 3 for learning)
  // Sprint 1-57 details: git log -- scrum.yaml, scrum.ts
  completed: [
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
    {
      number: 59,
      pbi_id: "PBI-077",
      goal: "Remove redundant Document.last_semantic_tokens storage - Tidy First structural cleanup",
      status: "done",
      subtasks: [],
    },
  ],

  retrospectives: [
    {
      sprint: 61,
      improvements: [
        {
          action:
            "Verify incremental path fully wired - ensure compute_incremental_tokens() is used when UseIncremental strategy selected",
          timing: "sprint",
          status: "completed",
          outcome: "Wired in Sprint 62 - compute_incremental_tokens() now invoked when UseIncremental selected",
        },
        {
          action:
            "Add integration test verifying incremental path reduces token computation work (not just merge logic)",
          timing: "sprint",
          status: "completed",
          outcome: "Added test_incremental_wiring.rs with 4 tests verifying preservation of cached tokens",
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
  ],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
