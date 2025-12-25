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

  // Completed PBIs: PBI-001 through PBI-078
  // For historical details: git log -- scrum.yaml, scrum.ts
  // Design reference: __ignored/semantic-token-performance.md
  product_backlog: [
    {
      id: "PBI-080",
      story: {
        role: "developer editing a large file",
        capability:
          "have the incremental tokenization path enabled using merge_tokens() for localized edits",
        benefit:
          "experience measurably faster highlighting updates (<20ms) for single-line edits in 1000+ line files",
      },
      acceptance_criteria: [
        {
          criterion:
            "semantic_tokens_full_delta uses merge_tokens() when previous_tree exists and is_large_structural_change() returns false",
          verification:
            "Integration test: verify incremental path is taken for small edits via log or metrics",
        },
        {
          criterion:
            "Benchmark confirms <20ms token update for single-line edit in 1000-line file",
          verification:
            "Benchmark test comparing incremental vs full tokenization timing",
        },
        {
          criterion:
            "Highlighting remains correct after incremental tokenization",
          verification:
            "E2E test: edit file, verify semantic tokens match full recomputation",
        },
      ],
      status: "ready",
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
    number: 61,
    pbi_id: "PBI-080",
    goal: "Enable incremental tokenization path using merge_tokens() so that localized edits achieve <20ms highlighting updates in large files",
    status: "in_progress",
    subtasks: [
      // Subtask 1: Unit test for incremental path decision logic
      {
        test: "Write unit test: incremental_path_chosen_when_small_change() - verify that when previous_tree exists and is_large_structural_change() returns false, the incremental path is selected",
        implementation: "Add decision logic function that returns IncrementalDecision enum (UseIncremental/UseFull) based on previous_tree presence and is_large_structural_change result",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Test location: src/analysis/incremental_tokens.rs", "This is a pure function test, no LSP integration yet"],
      },
      // Subtask 2: Unit test for merge_tokens integration with changed_ranges
      {
        test: "Write unit test: merge_tokens_uses_changed_ranges() - verify merge_tokens correctly integrates with get_changed_ranges() and changed_ranges_to_lines() to identify affected regions",
        implementation: "Create helper function compute_incremental_tokens() that orchestrates get_changed_ranges, changed_ranges_to_lines, and merge_tokens",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Test with actual tree-sitter parse trees", "Verify tokens outside changed region are preserved"],
      },
      // Subtask 3: Integration test for handle_semantic_tokens_full_delta with incremental path
      {
        test: "Write integration test: handle_semantic_tokens_full_delta_uses_incremental_path() - verify that handle_semantic_tokens_full_delta uses incremental tokenization when conditions are met",
        implementation: "Modify handle_semantic_tokens_full_delta signature to accept previous_tree and integrate incremental tokenization logic",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Requires signature change to accept previous_tree", "Fall back to full tokenization when incremental not possible"],
      },
      // Subtask 4: Integration test for lsp_impl incremental path
      {
        test: "Write integration test: lsp_impl_invokes_incremental_tokenization() - verify semantic_tokens_full_delta in lsp_impl passes previous_tree to handler and uses incremental path for small edits",
        implementation: "Wire up lsp_impl.rs to pass doc.previous_tree() to handle_semantic_tokens_full_delta and invoke incremental path",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Integration point: src/lsp/lsp_impl.rs lines 964-983", "Replace logging-only code with actual incremental call"],
      },
      // Subtask 5: Correctness test - highlighting matches full recomputation
      {
        test: "Write E2E test: incremental_tokens_match_full_recomputation() - edit a file, verify incremental semantic tokens match what full recomputation would produce",
        implementation: "No new implementation needed - this validates correctness of the integrated system",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["AC: Highlighting remains correct after incremental tokenization", "Compare incremental result with full tokenization result"],
      },
      // Subtask 6: Benchmark test for performance requirement
      {
        test: "Write benchmark test: incremental_tokenization_under_20ms() - measure token update latency for single-line edit in 1000-line file, assert <20ms",
        implementation: "Create benchmark infrastructure if needed, run incremental vs full comparison",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["AC: Benchmark confirms <20ms token update for single-line edit in 1000-line file", "Use criterion or simple timing with assertions"],
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
  // Sprint 1-56 details: git log -- scrum.yaml, scrum.ts
  completed: [
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
    {
      number: 58,
      pbi_id: "PBI-076",
      goal: "Fix semantic token cache invalidation on document edit",
      status: "done",
      subtasks: [],
    },
  ],

  retrospectives: [],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
