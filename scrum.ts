// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Lua developer editing markdown",
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

  // Completed PBIs: PBI-001-140 (Sprint 1-113), PBI-155-161 (124-130), PBI-178-180a (133-135), PBI-184 (136), PBI-181 (137), PBI-185 (138), PBI-187 (139), PBI-180b (140), PBI-190 (141), PBI-191 (142), PBI-192 (143), PBI-301 (144)
  // Deferred: PBI-091, PBI-107 | Removed: PBI-163-177 | Superseded: PBI-183 | Cancelled: Sprint 139 PBI-180b attempt
  // Sprint 139-144: All sprints DONE (Sprint 144: async bridge walking skeleton - 7 tests pass, all ACs satisfied)
  // Walking Skeleton PBIs: PBI-302 through PBI-304 (Ready)
  product_backlog: [
    // ============================================================
    // Walking Skeleton: Incremental Bridge Feature Development
    // Each PBI delivers independently testable, demonstrable value
    // ============================================================

    // --- PBI-301: Basic Bridge Attachment (Minimal Viable Slice) ---
    {
      id: "PBI-301",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "notice lua-language-server is attached to Lua code block in markdown",
        benefit:
          "I can expect lua-language-server features",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given a markdown file with Lua code block, when treesitter-ls opens the file, then lua-language-server process is spawned as a child process",
          verification:
            "Unit test: verify child process spawned with correct command (lua-language-server)",
        },
        {
          criterion:
            "Given lua-language-server is spawned, when initialization completes, then treesitter-ls logs 'lua-language-server initialized' or similar confirmation",
          verification:
            "Integration test: check log output contains initialization confirmation",
        },
        {
          criterion:
            "Given lua-language-server is running, when treesitter-ls terminates, then lua-language-server child process is also terminated",
          verification:
            "Integration test: verify no orphan lua-language-server processes after treesitter-ls exit",
        },
      ],
      status: "done",
      refinement_notes: [
        "Thinnest possible slice - just spawn the server and confirm it's running",
        "lua-language-server chosen over rust-analyzer: faster init, simpler config, common use case (Neovim docs)",
        "No initialization window required (assume notifications/requests start after initialized)",
        "No shutdown required (assume child process lua-language-server is killed by termination of parent process treesitter-ls)",
        "Assume single language server support. No need of language server pool yet",
        "Editor shows 'lua-language-server spawned' or similar in logs",
        "No useful features yet (no hover, no completions, ...)",
        "Proves the architecture works",
      ],
    },

    // --- PBI-302: Hover Feature ---
    {
      id: "PBI-302",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "hover from lua-language-server in Lua code block in markdown",
        benefit:
          "I understand the details of variables and functions",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given a Lua code block with a local variable, when I hover over the variable name, then I see type information from lua-language-server",
          verification:
            "E2E test: hover on 'local x = 1' variable shows type annotation",
        },
        {
          criterion:
            "Given a Lua code block with a function call, when I hover over the function name, then I see function signature from lua-language-server",
          verification:
            "E2E test: hover on 'print' shows function signature",
        },
        {
          criterion:
            "Given cursor position in host markdown, when hover request is sent, then position is correctly mapped to virtual Lua document",
          verification:
            "Unit test: verify position transformation from markdown line/col to Lua line/col",
        },
        {
          criterion:
            "Given hover response from lua-language-server, when response is returned to client, then URI is transformed back to original markdown URI",
          verification:
            "Unit test: verify URI transformation from virtual Lua URI to markdown URI",
        },
      ],
      status: "ready",
      refinement_notes: [
        "First actual LSP feature on top of the basic bridge",
        "Proves request/response flow works (textDocument/hover)",
        "Requires position mapping: host markdown position -> virtual Lua document position",
        "Requires URI transformation: markdown URI <-> virtual Lua URI",
        "Builds on PBI-301 (basic bridge attachment)",
      ],
    },

    // --- PBI-303: Completions Feature ---
    {
      id: "PBI-303",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "completions from lua-language-server in Lua code block in markdown",
        benefit:
          "I can write Lua code faster with autocomplete",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given a Lua code block, when I type a partial identifier, then I see completion suggestions from lua-language-server",
          verification:
            "E2E test: typing 'pri' in Lua block shows 'print' completion",
        },
        {
          criterion:
            "Given a Lua code block with a table variable, when I type the table name followed by dot, then I see member completions",
          verification:
            "E2E test: typing 'string.' shows string library methods",
        },
        {
          criterion:
            "Given document changes in markdown, when textDocument/didOpen is sent, then virtual Lua document is synced to lua-language-server",
          verification:
            "Unit test: verify didOpen notification sent with correct virtual document content",
        },
        {
          criterion:
            "Given document changes in markdown, when textDocument/didChange is sent, then virtual Lua document is updated in lua-language-server",
          verification:
            "Unit test: verify didChange notification sent with correct incremental changes",
        },
        {
          criterion:
            "Given completion items from lua-language-server, when items are returned to client, then they are relevant to the Lua context",
          verification:
            "E2E test: completions include Lua-specific items (local, function, table, etc.)",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Second LSP feature after hover",
        "Proves notifications work (textDocument/didOpen, textDocument/didChange)",
        "Requires document synchronization between host markdown and virtual Lua document",
        "Completion items should be filtered/relevant to Lua context",
        "Builds on PBI-301 (bridge) and PBI-302 (position/URI mapping)",
      ],
    },

    // --- PBI-304: Non-Blocking Initialization ---
    {
      id: "PBI-304",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "bridge server initialization never blocks treesitter-ls functionality",
        benefit:
          "I can edit markdown regardless of lua-language-server state",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given lua-language-server is starting up, when treesitter-ls receives any request, then treesitter-ls responds without blocking",
          verification:
            "Integration test: send requests during lua-ls startup, verify response time < 100ms",
        },
        {
          criterion:
            "Given lua-language-server is initializing, when hover/completion request is sent for Lua block, then an appropriate error response is returned (not timeout)",
          verification:
            "Unit test: verify error response with message indicating bridge not ready",
        },
        {
          criterion:
            "Given lua-language-server initialization completes, when bridge transitions to ready state, then subsequent requests are handled normally",
          verification:
            "Integration test: verify requests succeed after initialization completes",
        },
        {
          criterion:
            "Given user is editing markdown, when lua-language-server is initializing, then markdown editing features (syntax highlighting, folding) continue to work",
          verification:
            "E2E test: verify treesitter-ls native features work during bridge initialization",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Add initialization window handling per ADR-0018",
        "lua-language-server has faster init than rust-analyzer, but still needs async handling",
        "treesitter-ls remains responsive during lua-language-server startup",
        "Requests to Lua bridge during initialization return appropriate errors (not hang/timeout)",
        "Improves user experience - no perceived lag when opening files",
        "Builds on PBI-301, PBI-302, PBI-303",
      ],
    },
  ],
  sprint: null,
  completed: [
    {
      number: 144,
      pbi_id: "PBI-301",
      goal: "Establish the foundational bridge architecture by spawning lua-language-server as a child process and confirming successful initialization, proving the async bridge concept works",
      status: "done",
      subtasks: [
        {
          test: "Unit test that src/lsp/bridge/mod.rs module compiles and exports AsyncBridgeConnection type",
          implementation: "Create minimal module structure with placeholder types",
          type: "structural",
          status: "completed",
          commits: [{ hash: "1393ded9", message: "feat(bridge): add AsyncBridgeConnection module structure", phase: "green" }],
          notes: ["Module organization following existing lsp/ structure"],
        },
        {
          test: "Unit test that AsyncBridgeConnection::spawn() spawns a child process with the given command",
          implementation: "Use tokio::process::Command to spawn lua-language-server with stdio pipes",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "a7116891", message: "feat(bridge): implement spawn() for child process creation", phase: "green" }],
          notes: ["ADR-0014: Use tokio::process for async I/O"],
        },
        {
          test: "Unit test that send_request() writes JSON-RPC message to stdin with proper Content-Length header",
          implementation: "Serialize LSP initialize request to JSON-RPC format, write to async stdin",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "4ff80258", message: "feat(bridge): implement write_message for JSON-RPC formatting", phase: "green" }],
          notes: ["LSP JSON-RPC format: Content-Length header + body"],
        },
        {
          test: "Unit test that reader task parses Content-Length header and reads JSON-RPC response body",
          implementation: "Spawn async reader task that reads from stdout, parses LSP messages",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "d48e9557", message: "feat(bridge): implement read_message for JSON-RPC parsing", phase: "green" }],
          notes: ["ADR-0014: Reader task with select! for read/shutdown"],
        },
        {
          test: "Unit test that response is routed to correct pending request via request ID",
          implementation: "Use DashMap<RequestId, oneshot::Sender> for pending request tracking",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "551917f1", message: "feat(bridge): implement pending request routing via request ID", phase: "green" }],
          notes: ["ADR-0014: Pending request lifecycle management"],
        },
        {
          test: "Integration test that spawning lua-language-server and sending initialize results in log message",
          implementation: "Wire up full initialization flow, add log output on successful initialize response",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "89a2e1f6", message: "test(bridge): add lua-language-server initialization integration test", phase: "green" }],
          notes: [
            "AC2: logs 'lua-language-server initialized' or similar confirmation",
            "Requires lua-language-server installed on test machine",
          ],
        },
        {
          test: "Integration test that dropping AsyncBridgeConnection terminates the child process",
          implementation: "Implement Drop trait to kill child process; verify no orphan processes",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "525661d9", message: "feat(bridge): implement Drop trait to terminate child process", phase: "green" }],
          notes: ["AC3: child process terminated when treesitter-ls terminates"],
        },
      ],
    },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  // Retrospectives (recent 4) | Sprints 1-139: git log -- scrum.yaml, scrum.ts
  retrospectives: [],
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
