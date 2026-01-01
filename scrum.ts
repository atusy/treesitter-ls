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

  // Completed PBIs: PBI-001 through PBI-134 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Deferred - infrastructure already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [],

  sprint: {
    number: 112,
    pbi_id: "PBI-135",
    goal: "Capture publishDiagnostics from bridged language servers, translate ranges using CacheableInjectionRegion, and forward to editor with host document URI",
    status: "in_progress",
    subtasks: [
      {
        test: "Unit test: DiagnosticCollector stores publishDiagnostics notification by virtual URI key",
        implementation: "Create DiagnosticCollector struct in src/lsp/bridge/text_document/diagnostic.rs with insert/get methods keyed by virtual URI",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "783e65e", message: "feat(bridge): add DiagnosticCollector for storing diagnostics by virtual URI", phase: "green" }],
        notes: ["Pattern: DashMap<Url, Vec<Diagnostic>> like DocumentStore", "Store raw diagnostics before translation"],
      },
      {
        test: "Unit test: translate_diagnostic_range converts virtual line 0 to host line matching injection start_row",
        implementation: "Add translate_diagnostic method to CacheableInjectionRegion that translates Diagnostic range using translate_virtual_to_host",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "7050353", message: "feat(injection): add translate_diagnostic method to CacheableInjectionRegion", phase: "green" }],
        notes: ["Reuse existing translate_virtual_to_host from injection.rs", "Handle both start and end positions of diagnostic range"],
      },
      {
        test: "Unit test: DiagnosticForwarder transforms diagnostics from virtual URI to host URI with translated ranges",
        implementation: "Create VirtualToHostRegistry that maps virtual URIs to host URIs and injection regions, with translate_diagnostics method",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: ["Renamed to VirtualToHostRegistry for clarity", "translate_diagnostics returns PublishDiagnosticsParams with host URI and translated ranges"],
      },
      {
        test: "Unit test: LanguageServerConnection captures publishDiagnostics notifications during response wait",
        implementation: "Extend read_response_for_id_with_notifications to capture and store publishDiagnostics in DiagnosticCollector",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Pattern: similar to $/progress capture in wait_for_indexing_with_notifications", "Store in connection-level DiagnosticCollector or return with response"],
      },
      {
        test: "Integration test: did_open triggers diagnostic collection and forwarding for injection regions",
        implementation: "Wire DiagnosticCollector and DiagnosticForwarder into did_open flow after eager_spawn_for_injections",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Call client.publish_diagnostics with forwarded diagnostics", "Handle multiple injection regions in same document"],
      },
      {
        test: "Integration test: did_change clears old diagnostics and re-collects for affected injection regions",
        implementation: "Clear DiagnosticCollector entries for document URI on did_change, re-send didChange to bridged servers, collect new diagnostics",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Pattern: similar to invalidate_overlapping_injection_caches", "Must clear before new diagnostics arrive to avoid stale data"],
      },
      {
        test: "E2E test: nvim_diagnostic_test.lua verifies Rust code block in Markdown shows rust-analyzer diagnostics",
        implementation: "Create E2E test that opens Markdown with Rust code block containing error, verifies nvim.diagnostic.get returns expected diagnostic",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Use pattern from existing E2E tests in tests/nvim/", "Verify diagnostic line matches host document position"],
      },
      {
        test: "E2E test: fixing error in code block removes corresponding diagnostic",
        implementation: "Extend E2E test to modify buffer to fix error, verify diagnostic is cleared after change",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Tests AC4: diagnostics cleared and re-collected on didChange", "Verify empty diagnostic list after fix"],
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

  // Historical sprints (recent 2) | Sprint 1-110: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 111, pbi_id: "PBI-134", goal: "Store virtual_file_path in AsyncLanguageServerPool so get_virtual_uri returns valid URIs", status: "done", subtasks: [] },
    { number: 110, pbi_id: "PBI-133", goal: "Verify DashMap lock safety with concurrent test and add safety documentation", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-109: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 111,
      improvements: [
        { action: "PR review from external tools (gemini-code-assist) caught real bug - continue using automated PR review for async bridge features", timing: "immediate", status: "completed", outcome: "gemini-code-assist identified get_virtual_uri always returning None; bug fixed in Sprint 111" },
        { action: "When implementing new async connection features, always verify the full request flow including stored state (virtual URIs, document versions) before marking complete", timing: "immediate", status: "completed", outcome: "Added test async_pool_stores_virtual_uri_after_connection to verify URI storage" },
        { action: "Add E2E test for async bridge hover feature to verify end-to-end flow works (unit test exists but no E2E coverage)", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 110,
      improvements: [
        { action: "Investigate root cause earlier when PBI assumes a bug exists - validate assumption before detailed implementation planning", timing: "immediate", status: "completed", outcome: "Sprint 110 refinement correctly pivoted from 'fix deadlock' to 'verify and document safety' when code was found already safe" },
        { action: "Document Rust's .and_then() pattern as key to DashMap safety - it consumes Ref guard before subsequent operations", timing: "immediate", status: "completed", outcome: "Lock safety comments added to DocumentStore methods explaining .and_then() pattern" },
        { action: "User hang issue investigation: bridge I/O timeout (Sprint 109) and DashMap (Sprint 110) ruled out - investigate tokio::spawn panics or other mutex contention as next step", timing: "product", status: "active", outcome: null },
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
