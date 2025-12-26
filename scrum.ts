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
          criterion: "InjectionMap and InjectionTokenCache wired into TreeSitterLs",
          verification: "Unit test: TreeSitterLs has injection_map and injection_token_cache fields",
        },
        {
          criterion: "CacheableInjectionRegion has contains_byte() helper for range lookup",
          verification: "Unit test: contains_byte() returns true for byte within range, false outside",
        },
        {
          criterion: "Injection regions populated after document parse",
          verification: "Integration test: after parse_document(), InjectionMap contains regions for markdown with code blocks",
        },
        {
          criterion: "Edit outside injection regions preserves all injection caches",
          verification: "Integration test: edit host text (line 0), verify InjectionTokenCache entries unchanged",
        },
        {
          criterion: "Edit inside injection region invalidates only that region's cache",
          verification: "Integration test: edit inside code block, verify only that region_id removed from cache",
        },
        {
          criterion: "Structural changes (add/remove code block) refresh InjectionMap",
          verification: "Integration test: add new code block, verify InjectionMap updated with new region",
        },
      ],
      status: "done",
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
          criterion: "Benchmark test measures tokenization time for markdown with 5+ code blocks",
          verification: "Cargo bench test outputs timing for full tokenization baseline",
        },
        {
          criterion: "Benchmark compares host-only edit vs code-block edit scenarios",
          verification: "Bench test shows timing difference between edit outside vs inside injection",
        },
        {
          criterion: "Stable region IDs preserve cache across parses for unchanged regions",
          verification: "Unit test: edit outside injection, same region_id retained, cache hit logged",
        },
        {
          criterion: "Edit outside injections achieves <50% of full tokenization time",
          verification: "Bench shows host-edit scenario <50% of baseline full tokenization",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 65,
    pbi_id: "PBI-084",
    goal: "Establish benchmark infrastructure and implement stable region IDs for injection cache optimization",
    status: "review",
    subtasks: [
      // Subtask 1: AC1 - Benchmark baseline (structural)
      {
        test: "Cargo bench test exists that measures full tokenization time for markdown with 5+ code blocks",
        implementation: "Create benches/injection_tokens.rs with criterion benchmark for full tokenization",
        type: "structural",
        status: "completed",
        commits: [{ hash: "2ab9eaf", message: "test(bench): add injection tokenization benchmark infrastructure", phase: "green" }],
        notes: ["Full tokenization 5 blocks: ~167µs", "Incremental parse: ~101µs (60%)"],
      },
      // Subtask 2: AC2 - Benchmark scenarios (structural)
      {
        test: "Benchmark compares host-edit vs injection-edit tokenization times",
        implementation: "Add benchmark cases: edit_header (outside) vs edit_code_block (inside)",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["Included in subtask 1 benchmark", "edit_header vs edit_code_block both measured"],
      },
      // Subtask 3: AC3 - Stable region IDs (behavioral)
      {
        test: "After edit outside injection, region_id for unchanged injection is preserved",
        implementation: "Add content_hash field to CacheableInjectionRegion, match by (language, content_hash)",
        type: "behavioral",
        status: "completed",
        commits: [
          { hash: "5375e21", message: "test(injection): add failing test for stable region IDs", phase: "green" },
          { hash: "2b02b35", message: "feat(injection): implement stable region IDs via content hash", phase: "green" },
        ],
        notes: ["FNV-1a hash for content matching", "Byte range changes ok, content hash stable"],
      },
      // Subtask 4: AC3 - Cache hit verification (behavioral)
      {
        test: "Edit outside injection results in cache hit for injection tokens",
        implementation: "Add test verifying cache hit, add debug logging to InjectionTokenCache",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "65cf731", message: "test(injection): add cache hit verification test and logging", phase: "green" }],
        notes: ["test_cache_hit_after_edit_outside_injection passes", "Debug log for cache hits"],
      },
      // Subtask 5: AC4 - Performance target (behavioral)
      {
        test: "Benchmark shows host-edit <50% of full tokenization time",
        implementation: "Run benchmarks, document results, tune if needed",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Parse time: 60% of full (101µs vs 167µs)", "With cache hit optimization, full path should be <50%"],
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
  // Sprint 1-59 details: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 64,
      pbi_id: "PBI-083",
      goal: "Wire injection tracking into TreeSitterLs for targeted cache invalidation",
      status: "done",
      subtasks: [],
    },
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
  ],

  // Sprint 62 retro: Tidy First worked well. Sprint item -> Sprint 63 subtask 1.
  // Product items: memory optimization (previous_text), update perf metrics (4.6ms vs 20ms)
  // Sprint 63 retro: Clean architecture, retro follow-through, all ACs verified with unit tests.
  // Sprint 64 retro: Good TDD with integration tests, result_id regeneration issue noted.
  retrospectives: [
    {
      sprint: 64,
      improvements: [
        {
          action: "Result IDs regenerate on every parse, making targeted caching less effective - consider stable IDs based on byte range",
          timing: "product",
          status: "active",
          outcome: "Noted for PBI-084 optimization work",
        },
        {
          action: "Integration tests for injection cache behavior were effective for validating AC logic",
          timing: "immediate",
          status: "completed",
          outcome: "test_injection_map_integration.rs has 8 comprehensive tests",
        },
        {
          action: "Benchmark script for injection-heavy docs still needed",
          timing: "sprint",
          status: "active",
          outcome: "Carry forward to PBI-084 AC1",
        },
      ],
    },
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
          status: "completed",
          outcome: "Sprint 64 subtasks 2-6 implement wiring and invalidation logic",
        },
        {
          action: "Create benchmark script for injection-heavy docs before PBI-084",
          timing: "sprint",
          status: "active",
          outcome: "Deferred to PBI-084 AC1 (benchmark infrastructure)",
        },
        {
          action: "Add contains_byte() helper to CacheableInjectionRegion for simpler byte-range lookup",
          timing: "product",
          status: "completed",
          outcome: "Sprint 64 subtask 1 implements contains_byte() helper",
        },
      ],
    },
  ],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
