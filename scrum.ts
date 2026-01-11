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
      id: "PBI-DIDCLOSE-FORWARDING",
      story: {
        role: "Lua developer editing markdown",
        capability: "I want to propagate close of the host document to the virtual documents attached to bridged downstream language servers",
        benefit: "So that I do not suffer from memory leaks",
      },
      acceptance_criteria: [
        {
          criterion: "didClose sent for all virtual documents when host closes",
          verification: "E2E test: close host document and verify downstream servers receive didClose for each virtual document",
        },
        {
          criterion: "host to virtual mapping recorded during didOpen",
          verification: "Unit test: after didOpen, host_to_virtual contains entry mapping host URI to OpenedVirtualDoc with language and virtual_uri",
        },
        {
          criterion: "document_versions tracking cleaned up",
          verification: "Unit test: after didClose, document_versions no longer contains entries for closed virtual documents",
        },
        {
          criterion: "Connection remains open after didClose",
          verification: "E2E test: after closing one host document, other host documents can still send requests to the same downstream server",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Data structure: OpenedVirtualDoc { language: String, virtual_uri: String }",
        "Add host_to_virtual: Mutex<HashMap<Url, Vec<OpenedVirtualDoc>>> to LanguageServerPool",
        "Do not close connection because other host documents may need it",
      ],
    },
  ],
  sprint: {
    number: 160,
    pbi_id: "PBI-DIDCLOSE-FORWARDING",
    goal: "Propagate didClose from host documents to virtual documents ensuring proper cleanup without closing connections",
    status: "review",
    subtasks: [
      {
        test: "Unit test: OpenedVirtualDoc struct and host_to_virtual field exist in LanguageServerPool",
        implementation: "Add OpenedVirtualDoc struct with language and virtual_uri fields; add host_to_virtual: Mutex<HashMap<Url, Vec<OpenedVirtualDoc>>> to LanguageServerPool; initialize in new()",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e427c42b", message: "feat(bridge): propagate didClose to downstream language servers", phase: "green" }],
        notes: ["Data structure foundation for tracking host→virtual mappings"],
      },
      {
        test: "Unit test: should_send_didopen records host→virtual mapping when didOpen is sent",
        implementation: "Add host_uri parameter to should_send_didopen; record mapping in host_to_virtual when returning true; update callers in hover.rs, completion.rs, signature_help.rs",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e427c42b", message: "feat(bridge): propagate didClose to downstream language servers", phase: "green" }],
        notes: ["Maps host document to its virtual documents for later cleanup"],
      },
      {
        test: "Unit test: build_bridge_didclose_notification creates valid DidCloseTextDocumentParams",
        implementation: "Add build_bridge_didclose_notification function in protocol.rs similar to existing didOpen/didChange builders",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Protocol helper not needed - send_didclose_notification builds notification inline from virtual_uri directly"],
      },
      {
        test: "Unit test: send_didclose_notification sends notification to correct language server",
        implementation: "Add send_didclose_notification method to LanguageServerPool that sends didClose without closing connection",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e427c42b", message: "feat(bridge): propagate didClose to downstream language servers", phase: "green" }],
        notes: ["Low-level sending mechanism; connection must remain open"],
      },
      {
        test: "Unit test: close_host_document looks up virtual docs, sends didClose for each, and cleans up tracking",
        implementation: "Add close_host_document method that looks up host_to_virtual, sends didClose for each virtual doc, removes entries from document_versions and host_to_virtual",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e427c42b", message: "feat(bridge): propagate didClose to downstream language servers", phase: "green" }],
        notes: ["Orchestration method for full cleanup flow"],
      },
      {
        test: "E2E test: closing host document triggers didClose to downstream servers for all virtual documents",
        implementation: "Wire close_host_document call in lsp_impl.rs::did_close; verify downstream servers receive didClose",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e427c42b", message: "feat(bridge): propagate didClose to downstream language servers", phase: "green" }],
        notes: ["Full integration from Neovim to downstream server", "e2e_didclose_forwarded_to_downstream_server test passes"],
      },
      {
        test: "E2E test: connection remains open after didClose allowing other host documents to use same server",
        implementation: "Verify that after closing one markdown file with Lua blocks, another markdown file can still use lua-language-server",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e427c42b", message: "feat(bridge): propagate didClose to downstream language servers", phase: "green" }],
        notes: ["Critical: connection lifecycle is independent of document lifecycle", "e2e_connection_remains_open_after_didclose test passes"],
      },
    ],
  },
  completed: [
    { number: 159, pbi_id: "PBI-STABLE-REGION-ID", goal: "Implement stable region_id for shared virtual document URIs across bridge features", status: "done", subtasks: [] },
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
    { sprint: 159, improvements: [
      { action: "User conversation as refinement tool", timing: "immediate", status: "completed", outcome: "Discussing didChange forwarding implementation revealed hidden technical debt in region_id calculation - conversation-driven discovery led to clear User Story and acceptance criteria" },
      { action: "Fix foundational issues before building on them", timing: "immediate", status: "completed", outcome: "Stable region_id is prerequisite for didClose forwarding - addressing technical debt enables future features rather than accumulating workarounds" },
      { action: "Check all similar code when fixing patterns", timing: "immediate", status: "completed", outcome: "Found signature_help.rs had same 'temp' hardcoded region_id issue as hover.rs and completion.rs - comprehensive fix benefited all three bridge features" },
      { action: "Per-language ordinal counting provides stable identifiers", timing: "immediate", status: "completed", outcome: "Format {language}-{ordinal} ensures inserting Python blocks between Lua blocks preserves lua-0, lua-1 ordinals - simple approach without complex heuristics" },
    ]},
    { sprint: 158, improvements: [
      { action: "Well-established patterns accelerate implementation", timing: "immediate", status: "completed", outcome: "Following hover.rs and completion.rs patterns made signature_help.rs straightforward - consistent structure across text_document/ features" },
      { action: "Simpler features validate pattern robustness", timing: "immediate", status: "completed", outcome: "SignatureHelp required no range transformation (unlike completion), proving pattern handles varying complexity levels" },
      { action: "Pattern template for remaining bridge features", timing: "immediate", status: "completed", outcome: "Established pattern: pool method + protocol helpers + lsp_impl integration + E2E test - ready for codeAction and definition" },
      { action: "TDD catches integration issues early", timing: "immediate", status: "completed", outcome: "E2E tests verified full bridge wiring including request ID passthrough from Sprint 157" },
    ]},
    { sprint: 157, improvements: [{ action: "Tower Service middleware for cross-cutting concerns", timing: "immediate", status: "completed", outcome: "RequestIdCapture wrapper + task-local storage" }] },
    { sprint: 156, improvements: [{ action: "Research sprints are valid outcomes", timing: "immediate", status: "completed", outcome: "Research led to Service wrapper discovery" }] },
    { sprint: 155, improvements: [{ action: "Box::pin for recursive async calls", timing: "immediate", status: "completed", outcome: "Recursive retry compiles" }] },
    { sprint: 154, improvements: [{ action: "Per-connection state via ConnectionHandle", timing: "immediate", status: "completed", outcome: "Race conditions fixed" }] },
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
