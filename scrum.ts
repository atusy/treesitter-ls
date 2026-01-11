// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Lua developer editing markdown",
  "lua/python developer editing markdown",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Implement LSP bridge to support essential language server features indirectly through bridging (ADR-0013, 0014, 0015, 0016, 0017, 0018)",
    success_metrics: [
      {
        metric: "ADR alignment",
        target:
          "Must align with Phase 1 of ADR-0013, 0014, 0015, 0016, 0017, 0018 in @docs/adr",
      },
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, codeAction, definition, hover",
      },
      {
        metric: "Modular architecture",
        target:
          "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  product_backlog: [
    {
      id: "PBI-STABLE-REGION-ID",
      story: {
        role: "lua/python developer editing markdown",
        capability: "I want bridge feature to open virtual documents with a stable URI across the features",
        benefit: "So that I experience more performant features",
      },
      acceptance_criteria: [
        {
          criterion: "Hover and completion use the same virtual URI for the same injection region",
          verification: "E2E test: hover then complete on same Lua block uses same document URI",
        },
        {
          criterion: "First access sends didOpen, subsequent access sends didChange (not didOpen again)",
          verification: "Unit test: verify protocol message sequence for repeated accesses",
        },
        {
          criterion: "region_id format is {language}-{ordinal} where ordinal is per-language count",
          verification: "Unit test: Lua-Lua-Python blocks produce lua-0, lua-1, python-0",
        },
        {
          criterion: "Ordinal is stable: adding Python injection does not shift Lua ordinals",
          verification: "Unit test: inserting Python block between Lua blocks preserves lua-0, lua-1",
        },
      ],
      status: "done",
    },
  ],
  sprint: {
    number: 159,
    pbi_id: "PBI-STABLE-REGION-ID",
    goal: "Implement stable region_id calculation for shared virtual document URIs across bridge features",
    status: "done",
    subtasks: [
      {
        test: "Unit test: calculate_region_id returns {language}-{ordinal} format",
        implementation: "Create calculate_region_id function in src/language/injection.rs that takes list of injections and target injection, returns ordinal based on same-language count",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "cb5d3d88", message: "feat(injection): add calculate_region_id for stable virtual document URIs", phase: "green" }],
        notes: ["Input: Vec<InjectionRegionInfo>, target &InjectionRegionInfo", "Output: String in format {language}-{ordinal}", "Ordinal is per-language (lua-0, lua-1, python-0)"],
      },
      {
        test: "Unit test: Lua-Python-Lua blocks produce lua-0, python-0, lua-1",
        implementation: "Ensure ordinal calculation iterates by document order and counts per language",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "37e1e5cf", message: "test(injection): verify Lua-Python-Lua blocks get correct ordinals", phase: "green" }],
        notes: ["First Lua block: lua-0", "Python block: python-0", "Second Lua block: lua-1"],
      },
      {
        test: "Unit test: inserting Python block between Lua blocks preserves lua-0, lua-1 ordinals",
        implementation: "Ordinal based on same-language blocks only, ignoring other languages",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "325bb12d", message: "test(injection): verify inserting Python preserves Lua ordinals", phase: "green" }],
        notes: ["Before: lua-0, lua-1", "After (with Python between): lua-0, python-0, lua-1", "Lua ordinals unchanged despite new block insertion"],
      },
      {
        test: "Unit test: find matching injection from position returns correct region",
        implementation: "Create find_injection_at_position helper that returns index and InjectionRegionInfo for a byte offset",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "bb25db60", message: "feat(injection): add find_injection_at_position helper", phase: "green" }],
        notes: ["Used by hover.rs and completion.rs to find target injection", "Input: byte offset, list of injections", "Output: Option<(usize, &InjectionRegionInfo)> - index needed for calculate_region_id"],
      },
      {
        test: "Unit test: hover uses calculated region_id instead of 'hover-temp'",
        implementation: "Update hover.rs to call calculate_region_id with collected injections and matching region",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "7a35eedf", message: "refactor(hover): use stable region_id instead of 'hover-temp'", phase: "green" }],
        notes: ["Replace CacheableInjectionRegion::from_region_info(region, 'hover-temp', &text)", "With calculated region_id from shared function"],
      },
      {
        test: "Unit test: completion uses calculated region_id instead of 'completion-temp'",
        implementation: "Update completion.rs to call calculate_region_id with collected injections and matching region",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "c6057be0", message: "refactor(completion): use stable region_id instead of 'completion-temp'", phase: "green" }],
        notes: ["Replace CacheableInjectionRegion::from_region_info(region, 'completion-temp', &text)", "With calculated region_id from shared function"],
      },
      {
        test: "E2E test: hover then complete on same Lua block uses same document URI",
        implementation: "Verify hover and completion share virtual document by checking didOpen/didChange sequence",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e03a175b", message: "test(e2e): verify stable region_id across hover and completion", phase: "green" }],
        notes: ["First access (hover): didOpen sent to downstream", "Second access (completion): didChange sent (not didOpen)", "Same region_id ensures same virtual URI"],
      },
    ],
  },
  completed: [
    { number: 158, pbi_id: "PBI-SIGNATURE-HELP-BRIDGE", goal: "Enable signature help bridging for Lua code blocks in markdown documents", status: "done", subtasks: [] },
    { number: 157, pbi_id: "PBI-REQUEST-ID-SERVICE-WRAPPER", goal: "Pass upstream request IDs to downstream servers via tower Service wrapper per ADR-0016", status: "done", subtasks: [] },
    { number: 156, pbi_id: "PBI-REQUEST-ID-PASSTHROUGH", goal: "Validate ADR-0016 request ID semantics (research sprint)", status: "done", subtasks: [] },
    { number: 155, pbi_id: "PBI-RETRY-FAILED-CONNECTION", goal: "Enable automatic retry when downstream server connection has failed", status: "done", subtasks: [] },
    { number: 154, pbi_id: "PBI-STATE-PER-CONNECTION", goal: "Move ConnectionState to per-connection ownership fixing race condition", status: "done", subtasks: [] },
    { number: 153, pbi_id: "PBI-WIRE-FAILED-STATE", goal: "Return REQUEST_FAILED when downstream server has failed initialization", status: "done", subtasks: [] },
    { number: 152, pbi_id: "PBI-REQUEST-FAILED-INIT", goal: "Return REQUEST_FAILED immediately during initialization instead of blocking", status: "done", subtasks: [] },
    { number: 151, pbi_id: "PBI-INIT-TIMEOUT", goal: "Add timeout to initialization to prevent infinite hang", status: "done", subtasks: [] },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [
    { sprint: 158, improvements: [
      { action: "Well-established patterns accelerate implementation", timing: "immediate", status: "completed", outcome: "Following hover.rs and completion.rs patterns made signature_help.rs straightforward - consistent structure across text_document/ features" },
      { action: "Simpler features validate pattern robustness", timing: "immediate", status: "completed", outcome: "SignatureHelp required no range transformation (unlike completion), proving pattern handles varying complexity levels" },
      { action: "Pattern template for remaining bridge features", timing: "immediate", status: "completed", outcome: "Established pattern: pool method + protocol helpers + lsp_impl integration + E2E test - ready for codeAction and definition" },
      { action: "TDD catches integration issues early", timing: "immediate", status: "completed", outcome: "E2E tests verified full bridge wiring including request ID passthrough from Sprint 157" },
    ]},
    { sprint: 157, improvements: [
      { action: "Tower Service middleware pattern for cross-cutting concerns", timing: "immediate", status: "completed", outcome: "RequestIdCapture wrapper injects behavior without modifying core handler logic" },
      { action: "Task-local storage for request-scoped context", timing: "immediate", status: "completed", outcome: "tokio::task_local! provides clean request-scoped state without parameter threading" },
      { action: "Validate framework capabilities before concluding 'impossible'", timing: "immediate", status: "completed", outcome: "Sprint 156 prematurely concluded tower-lsp limitations; Service layer approach discovered via user feedback" },
      { action: "Fixed ID for internal requests", timing: "immediate", status: "completed", outcome: "Initialize handshake uses ID=0 for bridge-originated requests vs upstream-originated" },
    ]},
    { sprint: 156, improvements: [
      { action: "Investigate framework constraints before planning", timing: "immediate", status: "completed", outcome: "tower-lsp LanguageServer trait doesn't expose IDs, but Service wrapper can" },
      { action: "Distinguish ADR intent vs literal interpretation", timing: "immediate", status: "completed", outcome: "ADR-0016 intent achievable via Service wrapper pattern" },
      { action: "Research sprints are valid outcomes", timing: "immediate", status: "completed", outcome: "Research led to Service wrapper discovery - PBI-REQUEST-ID-SERVICE-WRAPPER created" },
    ]},
    { sprint: 155, improvements: [
      { action: "Box::pin for recursive async calls", timing: "immediate", status: "completed", outcome: "Recursive retry compiles" },
    ]},
    { sprint: 154, improvements: [
      { action: "Per-connection state via ConnectionHandle", timing: "immediate", status: "completed", outcome: "Race conditions fixed" },
    ]},
  ],
};

// Type Definitions (DO NOT MODIFY) =============================================
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
  refinement_notes?: string[];
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
