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

  // Completed PBIs: PBI-001 through PBI-071
  // For historical details: git log -- scrum.yaml, scrum.ts
  // Design reference: __ignored/semantic-token-performance.md
  product_backlog: [
    // Phase 1: Foundation (from design doc)
    {
      id: "PBI-072",
      story: {
        role: "treesitter-ls server",
        capability:
          "use atomic sequential result_id generation instead of content hash",
        benefit:
          "result_id generation is faster and simpler, eliminating hash computation overhead",
      },
      acceptance_criteria: [
        {
          criterion: "next_result_id() returns monotonically increasing string IDs",
          verification: "cargo test test_next_result_id_monotonic",
        },
        {
          criterion: "Concurrent calls return unique IDs (no duplicates)",
          verification: "cargo test test_next_result_id_concurrent",
        },
        {
          criterion:
            "semantic_tokens_full result contains result_id from next_result_id()",
          verification: "cargo test test_semantic_tokens_full_uses_atomic_id",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-073",
      story: {
        role: "treesitter-ls server",
        capability:
          "calculate semantic token deltas using prefix-suffix matching",
        benefit:
          "delta payload size is minimized when changes occur in the middle of the document",
      },
      acceptance_criteria: [
        {
          criterion: "diff_tokens finds common suffix after prefix",
          verification: "cargo test test_diff_tokens_suffix_matching",
        },
        {
          criterion: "Line insertion invalidates suffix optimization (PBI-077 safety)",
          verification: "cargo test test_diff_tokens_line_insertion_no_suffix",
        },
        {
          criterion: "Same-line edit preserves suffix optimization",
          verification: "cargo test test_diff_tokens_same_line_edit_suffix",
        },
        {
          criterion: "Empty delta when tokens unchanged",
          verification: "cargo test test_diff_tokens_no_change",
        },
      ],
      status: "done",
    },
    // Phase 2: Caching
    {
      id: "PBI-074",
      story: {
        role: "treesitter-ls server",
        capability:
          "cache semantic tokens with validation metadata in a dedicated cache",
        benefit:
          "repeated delta requests without changes return instantly without recomputation",
      },
      acceptance_criteria: [
        {
          criterion: "SemanticTokenCache stores tokens keyed by URL",
          verification: "cargo test test_semantic_cache_store_retrieve",
        },
        {
          criterion: "get_if_valid returns None when result_id mismatches",
          verification: "cargo test test_semantic_cache_invalid_result_id",
        },
        {
          criterion: "Cache entries removed on document close",
          verification: "cargo test test_semantic_cache_remove_on_close",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-075",
      story: {
        role: "treesitter-ls server",
        capability:
          "integrate SemanticTokenCache into LSP handlers for delta requests",
        benefit:
          "delta requests use cached previous tokens for efficient comparison",
      },
      acceptance_criteria: [
        {
          criterion:
            "TreeSitterLs has semantic_cache field of type SemanticTokenCache",
          verification: "code review: field exists in struct",
        },
        {
          criterion:
            "semantic_tokens_full stores result in SemanticTokenCache",
          verification: "code review: store called after computing tokens",
        },
        {
          criterion:
            "semantic_tokens_full_delta uses get_if_valid for previous tokens",
          verification: "code review: get_if_valid called with result_id",
        },
        {
          criterion: "did_close removes entry from SemanticTokenCache",
          verification: "code review: remove called in did_close",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 57,
    pbi_id: "PBI-075",
    goal: "Integrate SemanticTokenCache into LSP handlers",
    status: "in_progress",
    subtasks: [
      {
        test: "Add semantic_cache field to TreeSitterLs struct",
        implementation: "Add SemanticTokenCache field and initialize in new()",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Use cache in semantic_tokens_full",
        implementation: "Call semantic_cache.store() after computing tokens",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Use cache in semantic_tokens_full_delta",
        implementation: "Call semantic_cache.get_if_valid() for previous tokens",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Clean up cache in did_close",
        implementation: "Call semantic_cache.remove() in did_close handler",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
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
  // Sprint 1-53 details: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 56,
      pbi_id: "PBI-074",
      goal: "Implement dedicated SemanticTokenCache with result_id validation",
      status: "done",
      subtasks: [],
    },
    {
      number: 55,
      pbi_id: "PBI-073",
      goal: "Implement prefix-suffix matching for semantic token deltas",
      status: "done",
      subtasks: [],
    },
    {
      number: 54,
      pbi_id: "PBI-072",
      goal: "Implement atomic sequential result_id generation for semantic tokens",
      status: "done",
      subtasks: [],
    },
  ],

  retrospectives: [],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
