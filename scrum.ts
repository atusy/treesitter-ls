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

  // Completed PBIs: PBI-001 through PBI-077
  // For historical details: git log -- scrum.yaml, scrum.ts
  // Design reference: __ignored/semantic-token-performance.md
  product_backlog: [
    {
      id: "PBI-078",
      story: {
        role: "developer editing a large file",
        capability:
          "have semantic tokens re-computed only for changed regions using Tree-sitter's changed_ranges() API",
        benefit:
          "experience faster highlighting updates for localized edits without full document re-tokenization",
      },
      acceptance_criteria: [
        {
          criterion:
            "Tree-sitter changed_ranges() API is called after incremental parsing to identify modified regions",
          verification:
            "Unit test verifies changed_ranges is queried when old and new trees are available",
        },
        {
          criterion:
            "Tokens outside changed regions are preserved from the previous result",
          verification:
            "Integration test shows tokens for unmodified code sections remain unchanged",
        },
        {
          criterion:
            "Heuristic determines when incremental tokenization is beneficial vs full re-tokenization",
          verification:
            "Unit test for heuristic: >10 changed ranges or >30% document change triggers full recompute",
        },
        {
          criterion:
            "Performance improvement measured for localized edits in large documents",
          verification:
            "Benchmark shows <20ms response for single-line edits in 1000+ line files",
        },
      ],
      status: "draft",
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

  sprint: null,

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
    {
      number: 57,
      pbi_id: "PBI-075",
      goal: "Integrate SemanticTokenCache into LSP handlers",
      status: "done",
      subtasks: [],
    },
  ],

  retrospectives: [],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
