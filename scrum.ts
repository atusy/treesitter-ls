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

  // Completed PBIs: PBI-001 through PBI-085
  // For historical details: git log -- scrum.yaml, scrum.ts
  // Recent: PBI-082 (injection tracking), PBI-083 (cache invalidation), PBI-084 (perf), PBI-085 (auto-install)
  product_backlog: [
    {
      id: "PBI-086",
      story: {
        role: "developer editing Markdown with Rust code blocks",
        capability:
          "go-to-definition within a Rust code block redirects to the symbol definition",
        benefit:
          "I can navigate code inside documentation without switching to a separate Rust file",
      },
      acceptance_criteria: [
        {
          criterion:
            "Cursor on a function call inside a Rust code block triggers textDocument/definition",
          verification:
            "make test_nvim_file FILE=tests/test_lsp_definition.lua",
        },
        {
          criterion:
            "treesitter-ls spawns rust-analyzer subprocess for the virtual document",
          verification:
            "pgrep -f rust-analyzer shows process spawned during test",
        },
        {
          criterion:
            "Position coordinates are translated between host and virtual document correctly",
          verification:
            "E2E test confirms cursor moves to line 4 (fn example definition)",
        },
        {
          criterion: "No user configuration required - works with default settings",
          verification:
            "Test runs without any treesitter-ls configuration for redirection",
        },
      ],
      status: "ready",
    },
  ],

  // Sprint 68: PBI-086 - LSP redirection for definition in injections
  sprint: {
    number: 68,
    pbi_id: "PBI-086",
    goal: "Enable go-to-definition navigation within Rust code blocks in Markdown documents",
    status: "in_progress",
    subtasks: [
      {
        test: "find_injection_at_position() returns injection region containing a given byte position",
        implementation: "Add function to InjectionMap or create new module to locate injection by position",
        type: "behavioral",
        status: "completed",
        commits: [
          { hash: "7177265", message: "feat(analysis): add find_at_position() to InjectionMap", phase: "green" },
        ],
        notes: [
          "Reuse CacheableInjectionRegion from injection.rs",
          "Pattern already used inline at line 471 of semantic_cache.rs",
          "Skipped refactor - code is clean, existing test code pattern is not worth changing",
        ],
      },
      {
        test: "extract_virtual_document() creates content string from injection byte range",
        implementation: "Extract injection content from host document text using byte_range",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: ["Virtual doc is just the text slice, no need for file I/O"],
      },
      {
        test: "translate_host_to_virtual() converts host Position to virtual document Position",
        implementation: "Map host line/col to virtual coordinates by subtracting injection start",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Host line 9 → virtual line 6 (9 - 3 = 6, where 3 is code block start)"],
      },
      {
        test: "translate_virtual_to_host() converts virtual Position back to host Position",
        implementation: "Map response position back by adding injection start offset",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Virtual line 1 → host line 4 (1 + 3 = 4)"],
      },
      {
        test: "goto_definition handler redirects to rust-analyzer for Rust injection regions",
        implementation: "Spawn rust-analyzer, send didOpen + definition request, translate response",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["E2E test verifies full flow: make test_nvim_file FILE=tests/test_lsp_definition.lua"],
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
  // Sprint 1-66 details: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 67,
      pbi_id: "PBI-085",
      goal: "Fix auto-install blocking issue by spawning install tasks and preserving host highlighting",
      status: "done",
      subtasks: [],
    },
    {
      number: 66,
      pbi_id: "PBI-084",
      goal: "Analyze semantic token handler for cache integration feasibility",
      status: "done",
      subtasks: [],
    },
    {
      number: 65,
      pbi_id: "PBI-084",
      goal: "Establish benchmark infrastructure and implement stable region IDs for injection cache optimization",
      status: "done",
      subtasks: [],
    },
  ],

  // Sprint 63-66 retro summaries: git log -- scrum.ts
  // Sprint 67 retro: Simple parameter fix addressed root cause; E2E tests deferred (invasive parser manipulation)
  retrospectives: [
    {
      sprint: 67,
      improvements: [
        {
          action: "Simple is_injection parameter effectively fixed the root cause without async complexity",
          timing: "immediate",
          status: "completed",
          outcome: "Avoided tokio::spawn complexity by fixing the actual bug (wrong re-parse language)",
        },
        {
          action: "E2E tests for auto-install are invasive (require parser deletion/reinstall)",
          timing: "product",
          status: "active",
          outcome: "Consider dedicated test environment with temporary parser directories for isolation",
        },
        {
          action: "Bug reports with clear symptom descriptions enable fast root cause analysis",
          timing: "immediate",
          status: "completed",
          outcome: "User's 'doc lang=no highlight, injection=all disappears' mapped directly to code flow",
        },
      ],
    },
    {
      sprint: 66,
      improvements: [
        {
          action: "Pure-function module (semantic.rs) is hard to wire with cache state - consider architecture alternatives",
          timing: "product",
          status: "active",
          outcome: "Future PBI: may need to pass cache as parameter or restructure module",
        },
        {
          action: "60% improvement from incremental parsing is substantial and ready for use",
          timing: "immediate",
          status: "completed",
          outcome: "PBI-084 marked done with documented limitation (AC4 at 60%, not <50%)",
        },
        {
          action: "Exploration-first sprints useful for understanding complexity before committing",
          timing: "immediate",
          status: "completed",
          outcome: "Subtask 1 (explore) revealed high refactoring cost, enabling informed scope decision",
        },
      ],
    },
    {
      sprint: 65,
      improvements: [
        {
          action: "Content hash (FNV-1a) proved effective for stable region matching - simpler than AST-based approaches",
          timing: "immediate",
          status: "completed",
          outcome: "CacheableInjectionRegion.content_hash enables cache reuse when document structure changes",
        },
        {
          action: "Criterion benchmarks provide clear baseline metrics for optimization work",
          timing: "immediate",
          status: "completed",
          outcome: "benches/injection_tokens.rs measures parse time (167µs full, 101µs incremental)",
        },
        {
          action: "Parse time at 60% of full tokenization - cache hit optimization needed for <50% target",
          timing: "product",
          status: "active",
          outcome: "Consider adding benchmark that includes full semantic token generation, not just parsing",
        },
        {
          action: "Debug logging for cache hits aids in verifying optimization effectiveness",
          timing: "immediate",
          status: "completed",
          outcome: "treesitter_ls::injection_cache target logs hits/misses at debug/trace level",
        },
      ],
    },
  ],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
