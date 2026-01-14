// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Lua developer editing markdown",
  "lua/python developer editing markdown",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement: "Improve LSP feature coverage via bridge",
    success_metrics: [
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, definition, typeDefinition, implementation, declaration, hover, references, document highlight, inlay hints, document link, document symbols, moniker, color presentation, rename",
      },
      {
        metric: "Modular architecture",
        target:
          "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage using treesitter-ls binary",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  product_backlog: [
    {
      id: "pbi-document-highlight",
      story: {
        role: "Lua developer editing markdown",
        capability: "highlight all occurrences of a symbol in a Lua code block",
        benefit: "I can quickly see where a variable is used within the same region",
      },
      acceptance_criteria: [
        {
          criterion:
            "Bridge forwards textDocument/documentHighlight requests to downstream Lua LS",
          verification:
            "E2E test: cursor on variable returns highlight ranges for all occurrences",
        },
        {
          criterion: "Response positions are transformed to host document coordinates",
          verification: "Unit test: region_start_line offset applied to all ranges",
        },
        {
          criterion: "Cross-region virtual URIs are filtered from response",
          verification:
            "Unit test: highlights with different virtual URI prefix are excluded",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Similar pattern to references.rs - returns DocumentHighlight[] with ranges",
        "Reuse transform_definition_response_to_host pattern for Location-like ranges",
        "Protocol function: build_bridge_document_highlight_request",
      ],
    },
    {
      id: "pbi-rename",
      story: {
        role: "Lua developer editing markdown",
        capability: "rename a symbol across all occurrences in a Lua code block",
        benefit: "I can refactor code safely without missing any references",
      },
      acceptance_criteria: [
        {
          criterion:
            "Bridge forwards textDocument/rename requests to downstream Lua LS",
          verification:
            "E2E test: rename request returns WorkspaceEdit with text edits",
        },
        {
          criterion:
            "WorkspaceEdit positions are transformed to host document coordinates",
          verification: "Unit test: all TextEdit ranges have region_start_line offset",
        },
        {
          criterion: "Only edits for the current virtual URI are included",
          verification:
            "Unit test: edits for other virtual URIs are filtered out",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Returns WorkspaceEdit with documentChanges or changes map",
        "Need new transform function for WorkspaceEdit response type",
        "Protocol function: build_bridge_rename_request",
      ],
    },
    {
      id: "pbi-document-link",
      story: {
        role: "Lua developer editing markdown",
        capability: "follow links in Lua code blocks (e.g., require paths)",
        benefit: "I can navigate to referenced modules directly from the code block",
      },
      acceptance_criteria: [
        {
          criterion:
            "Bridge forwards textDocument/documentLink requests to downstream Lua LS",
          verification: "E2E test: require statement returns clickable link",
        },
        {
          criterion: "Link ranges are transformed to host document coordinates",
          verification: "Unit test: region_start_line offset applied to link ranges",
        },
        {
          criterion: "Link targets remain unchanged (external URIs)",
          verification: "Unit test: target URIs preserved as-is from downstream",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Returns DocumentLink[] with range and optional target",
        "Only range transformation needed, target is external URI",
        "Protocol function: build_bridge_document_link_request",
      ],
    },
    {
      id: "pbi-document-symbols",
      story: {
        role: "Lua developer editing markdown",
        capability: "see outline of symbols defined in a Lua code block",
        benefit: "I can navigate to functions and variables within the code block",
      },
      acceptance_criteria: [
        {
          criterion:
            "Bridge forwards textDocument/documentSymbol requests to downstream Lua LS",
          verification: "E2E test: function definitions appear in symbol list",
        },
        {
          criterion:
            "Symbol ranges are transformed to host document coordinates",
          verification:
            "Unit test: region_start_line offset applied to symbol and selection ranges",
        },
        {
          criterion:
            "Hierarchical symbol structure (children) is preserved with transformed ranges",
          verification:
            "Unit test: nested symbols maintain parent-child relationships",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Returns DocumentSymbol[] (hierarchical) or SymbolInformation[] (flat)",
        "Need recursive transformation for nested DocumentSymbol children",
        "Protocol function: build_bridge_document_symbol_request",
      ],
    },
    {
      id: "pbi-inlay-hints",
      story: {
        role: "Lua developer editing markdown",
        capability: "see inline type hints in Lua code blocks",
        benefit:
          "I can understand variable types without hovering over each symbol",
      },
      acceptance_criteria: [
        {
          criterion:
            "Bridge forwards textDocument/inlayHint requests to downstream Lua LS",
          verification: "E2E test: type annotations appear inline for variables",
        },
        {
          criterion:
            "Hint positions are transformed to host document coordinates",
          verification: "Unit test: region_start_line offset applied to hint positions",
        },
        {
          criterion: "Request range is transformed to virtual document coordinates",
          verification:
            "Unit test: visible range sent to downstream is offset by -region_start_line",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Request includes range parameter (visible viewport)",
        "Returns InlayHint[] with position and label",
        "Both request range and response positions need transformation",
        "Protocol function: build_bridge_inlay_hint_request",
      ],
    },
    {
      id: "pbi-color-presentation",
      story: {
        role: "lua/python developer editing markdown",
        capability: "pick and edit color values in code blocks",
        benefit: "I can visually edit colors without memorizing hex codes",
      },
      acceptance_criteria: [
        {
          criterion:
            "Bridge forwards textDocument/colorPresentation requests to downstream LS",
          verification:
            "E2E test: color picker returns valid color format options",
        },
        {
          criterion: "Request range is transformed to virtual document coordinates",
          verification:
            "Unit test: color range sent to downstream is offset correctly",
        },
        {
          criterion:
            "Response textEdit ranges are transformed to host coordinates",
          verification:
            "Unit test: edit ranges have region_start_line offset applied",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Requires textDocument/documentColor first (returns color locations)",
        "colorPresentation takes Color + range, returns ColorPresentation[]",
        "May need both documentColor and colorPresentation bridge handlers",
        "Protocol functions: build_bridge_document_color_request, build_bridge_color_presentation_request",
      ],
    },
    {
      id: "pbi-moniker",
      story: {
        role: "lua/python developer editing markdown",
        capability: "get unique symbol identifiers for cross-project navigation",
        benefit:
          "I can integrate with symbol indexing tools for large codebases",
      },
      acceptance_criteria: [
        {
          criterion:
            "Bridge forwards textDocument/moniker requests to downstream LS",
          verification: "E2E test: symbol at cursor returns moniker identifier",
        },
        {
          criterion: "Moniker response is passed through unchanged",
          verification:
            "Unit test: scheme, identifier, unique, kind fields preserved",
        },
        {
          criterion: "Request position is transformed to virtual coordinates",
          verification:
            "Unit test: cursor position offset by -region_start_line",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Returns Moniker[] with scheme, identifier, unique, kind",
        "Response contains no position/range data, only pass-through needed",
        "Less commonly supported by language servers",
        "Protocol function: build_bridge_moniker_request",
      ],
    },
  ],
  sprint: {
    number: 1,
    pbi_id: "pbi-document-highlight",
    goal: "Enable users to highlight all occurrences of a symbol within a Lua code block by bridging textDocument/documentHighlight to downstream language servers",
    status: "in_progress",
    subtasks: [
      {
        test: "Unit test: build_bridge_document_highlight_request creates valid JSON-RPC request with virtual URI and translated position",
        implementation: "Add build_bridge_document_highlight_request function in protocol.rs using build_position_based_request helper",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "9f025f6e", message: "feat(protocol): add build_bridge_document_highlight_request function", phase: "green" }],
        notes: [
          "Follow same pattern as build_bridge_references_request",
          "Method name: textDocument/documentHighlight",
          "No additional parameters needed (unlike references which has includeDeclaration)",
        ],
      },
      {
        test: "Unit test: transform_document_highlight_response_to_host transforms DocumentHighlight[] ranges by adding region_start_line offset",
        implementation: "Add transform_document_highlight_response_to_host function in protocol.rs that transforms ranges in DocumentHighlight array",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "f68b2526", message: "feat(protocol): add transform_document_highlight_response_to_host", phase: "green" }],
        notes: [
          "Response format: DocumentHighlight[] where each item has range and optional kind",
          "Transform each range.start.line and range.end.line by adding region_start_line",
          "Reuse transform_range helper function",
          "Filter cross-region virtual URIs using ResponseTransformContext pattern from definition",
        ],
      },
      {
        test: "Unit test: transform filters out DocumentHighlight items with cross-region virtual URIs",
        implementation: "N/A - DocumentHighlight per LSP spec has no URI field, only range+kind. Cross-region filtering not applicable.",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: [
          "ANALYSIS: DocumentHighlight per LSP spec has NO URI field - only range and optional kind",
          "Unlike Location[] (used by definition/references), DocumentHighlight[] cannot reference other documents",
          "Cross-region filtering is therefore not applicable to document highlights",
          "The existing transform_document_highlight_response_to_host is complete as-is",
        ],
      },
      {
        test: "Integration test: send_document_highlight_request returns transformed highlights from downstream LS",
        implementation: "Add document_highlight.rs module with send_document_highlight_request method on LanguageServerPool",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [
          "Create src/lsp/bridge/text_document/document_highlight.rs",
          "Follow references.rs pattern: get connection, send didOpen if needed, send request, transform response",
          "Add module declaration to text_document.rs",
        ],
      },
      {
        test: "Integration test: document_highlight_impl bridges request when cursor is in injection region",
        implementation: "Add document_highlight_impl method in lsp_impl/text_document and wire to LanguageServer trait",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Add document_highlight_impl in lsp_impl/text_document/document_highlight.rs",
          "Wire document_highlight method in LanguageServer trait implementation",
          "Add documentHighlightProvider capability in initialize response",
          "Follow pattern from references_impl",
        ],
      },
      {
        test: "E2E test: textDocument/documentHighlight in Lua code block returns highlights for variable occurrences",
        implementation: "Add E2E test in tests/test_e2e_bridge.rs verifying end-to-end document highlight flow",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Test markdown file with Lua code block containing variable used multiple times",
          "Send documentHighlight request at variable position",
          "Verify response contains highlight ranges for all occurrences",
          "Verify ranges are in host document coordinates (not virtual)",
        ],
      },
    ],
  },
  completed: [],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
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
