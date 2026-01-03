// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
  "treesitter-ls user managing configurations",
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

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113), PBI-155 (Sprint 124), PBI-156 (Sprint 125), PBI-157 (Sprint 126) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    {
      id: "PBI-161",
      story: {
        role: "treesitter-ls user managing configurations",
        capability: "update ADR-0010 and ADR-0011 to match actual implementation behavior",
        benefit: "documentation accurately reflects how the system works and prevents user confusion",
      },
      acceptance_criteria: [
        {
          criterion: "ADR-0010 query type inference examples updated to show exact filename matching only (highlights.scm, locals.scm, injections.scm)",
          verification: "ADR-0010 examples no longer show pattern matching like *highlights*.scm",
        },
        {
          criterion: "ADR-0010 legacy field merge behavior documented as prioritization not append",
          verification: "ADR-0010 states that queries field takes complete precedence when present, legacy fields ignored",
        },
        {
          criterion: "ADR-0011 removes outdated (future) tags from implemented features",
          verification: "languages, languages.{lang}.bridge, and languageServers no longer marked as (future)",
        },
      ],
      status: "draft",
    },
    {
      id: "PBI-160",
      story: {
        role: "treesitter-ls user managing configurations",
        capability: "use a named constant for wildcard key instead of magic string",
        benefit: "wildcard key is defined in one place preventing typos and making refactoring easier",
      },
      acceptance_criteria: [
        {
          criterion: "Wildcard constant defined in config module (pub const WILDCARD_KEY: &str = \"_\")",
          verification: "grep finds constant definition in src/config.rs or src/config/settings.rs",
        },
        {
          criterion: "All map.get(\"_\") calls replaced with map.get(WILDCARD_KEY)",
          verification: "grep confirms no remaining literal \"_\" string in wildcard resolution functions",
        },
        {
          criterion: "All existing tests continue to pass",
          verification: "make test passes",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-159",
      story: {
        role: "treesitter-ls user managing configurations",
        capability: "have coordinator query loading tested with unit tests",
        benefit: "the unified queries field integration is verified to work correctly",
      },
      acceptance_criteria: [
        {
          criterion: "Test verifies QueryItem with explicit kind field loads correctly",
          verification: "Unit test creates QueryItem with kind: Some(QueryKind::Highlights) and verifies it loads as highlights query",
        },
        {
          criterion: "Test verifies QueryItem without kind uses inference from filename",
          verification: "Unit test creates QueryItem with path ending in highlights.scm and verifies kind is inferred",
        },
        {
          criterion: "Test verifies unknown patterns are silently skipped",
          verification: "Unit test creates QueryItem with path rust-custom.scm and verifies no error occurs",
        },
        {
          criterion: "Test verifies queries are grouped correctly by type",
          verification: "Unit test with mixed QueryItems verifies highlights, locals, and injections are loaded separately",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-158",
      story: {
        role: "treesitter-ls user managing configurations",
        capability: "have XDG_CONFIG_HOME path validated before use",
        benefit: "malicious environment variables cannot cause path traversal attacks",
      },
      acceptance_criteria: [
        {
          criterion: "user_config_path validates XDG_CONFIG_HOME is absolute",
          verification: "Unit test with relative path XDG_CONFIG_HOME returns None instead of using invalid path",
        },
        {
          criterion: "user_config_path canonicalizes XDG_CONFIG_HOME to prevent traversal",
          verification: "Unit test verifies symlinks are resolved and .. components are eliminated",
        },
        {
          criterion: "Invalid XDG_CONFIG_HOME logs warning and falls back to ~/.config",
          verification: "Unit test verifies warning is logged and fallback path is used",
        },
      ],
      status: "done",
    },
    // Future: PBI-147 (hover wait), PBI-141/142/143 (async bridge methods)
    // ADR-0010: PBI-151 (118), PBI-150 (119), PBI-149 (120) | ADR-0011: PBI-152-155 (121-124)
  ],
  sprint: null,
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },
  // Historical sprints (recent 2) | Sprint 1-125: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 127, pbi_id: "PBI-158", goal: "Validate XDG_CONFIG_HOME to prevent path traversal attacks", status: "done", subtasks: [], commit: "cd7f4ec" },
    { number: 126, pbi_id: "PBI-157", goal: "Fix initialization_options shallow merge bug (ADR-0010 compliance)", status: "done", subtasks: [] },
  ],
  // Retrospectives (recent 2)
  retrospectives: [
    { sprint: 126, improvements: [
      { action: "Clarify CI/setup documentation for deps/treesitter requirement to prevent initial test failures", timing: "immediate", status: "active", outcome: null },
      { action: "Integrate code review feedback earlier in development cycle (before implementation completion)", timing: "sprint", status: "active", outcome: null },
      { action: "Extract and document reusable deep_merge_json pattern for other JSON merge use cases in codebase", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 125, improvements: [
      { action: "Document domain vs serialization type distinction in ADR: LanguageSettings (domain) vs LanguageConfig (serialization) separation rationale", timing: "immediate", status: "active", outcome: null },
      { action: "Consider introducing explicit conversion trait between domain and serialization types for type safety", timing: "product", status: "active", outcome: null },
    ] },
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
