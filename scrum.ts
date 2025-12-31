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
          "Support completion, signatureHelp, references, rename, codeAction, formatting",
      },
      {
        metric: "Modular architecture",
        target: "Bridge module split into per-feature files for maintainability",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-120 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    {
      id: "PBI-121",
      story: {
        role: "Rustacean editing Markdown",
        capability:
          "maintain and extend lsp_impl.rs through a modular file structure where each LSP text document feature lives in its own file",
        benefit:
          "the codebase is easier to navigate, understand, and modify without risk of merge conflicts",
      },
      acceptance_criteria: [
        {
          criterion:
            "Phase 1: semantic_tokens methods (semantic_tokens_full, semantic_tokens_full_delta, semantic_tokens_range) extracted to lsp_impl/text_document/semantic_tokens.rs",
          verification:
            "All existing tests pass; module structure: lsp_impl.rs delegates to lsp_impl/text_document/semantic_tokens.rs",
        },
        {
          criterion:
            "Phase 2: completion method extracted to lsp_impl/text_document/completion.rs",
          verification:
            "All existing tests pass; completion logic isolated in dedicated module",
        },
        {
          criterion:
            "Phase 3: hover method extracted to lsp_impl/text_document/hover.rs",
          verification:
            "All existing tests pass; hover logic isolated in dedicated module",
        },
        {
          criterion:
            "Phase 4: goto_definition method extracted to lsp_impl/text_document/definition.rs",
          verification:
            "All existing tests pass; definition logic isolated in dedicated module",
        },
        {
          criterion:
            "Phase 5: references method extracted to lsp_impl/text_document/references.rs",
          verification:
            "All existing tests pass; references logic isolated in dedicated module",
        },
        {
          criterion:
            "Phase 6: rename method extracted to lsp_impl/text_document/rename.rs",
          verification:
            "All existing tests pass; rename logic isolated in dedicated module",
        },
        {
          criterion:
            "Phase 7: formatting method extracted to lsp_impl/text_document/formatting.rs",
          verification:
            "All existing tests pass; formatting logic isolated in dedicated module",
        },
        {
          criterion:
            "Phase 8: code_action method extracted to lsp_impl/text_document/code_action.rs",
          verification:
            "All existing tests pass; code_action logic isolated in dedicated module",
        },
        {
          criterion:
            "Phase 9: selection_range and signature_help extracted to respective modules",
          verification:
            "All existing tests pass; final text_document submodules complete with mod.rs re-exports",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-122",
      story: {
        role: "Rustacean editing Markdown",
        capability:
          "configure bridge servers at the top-level 'languageServers' field instead of nested 'bridge.servers'",
        benefit:
          "the config structure is flatter and the field name clearly indicates LSP servers",
      },
      acceptance_criteria: [
        {
          criterion:
            "Top-level 'languageServers' field added to TreeSitterSettings schema",
          verification:
            "Config with languageServers.rust-analyzer works; unit tests verify deserialization",
        },
        {
          criterion:
            "bridge.servers deprecated but still functional (backwards compatibility)",
          verification:
            "Config with bridge.servers continues to work; effective_language_servers() returns merged result",
        },
        {
          criterion:
            "Deprecation warning logged when bridge.servers is used",
          verification:
            "log_deprecation_warnings() warns about bridge.servers usage",
        },
        {
          criterion:
            "Documentation updated with migration guide",
          verification:
            "README.md shows languageServers at top level with Before/After examples",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-123",
      story: {
        role: "Rustacean editing Markdown",
        capability:
          "configure bridge enabled/disabled per injection language with cascading defaults using '_' wildcard pattern",
        benefit:
          "I can set global defaults once and override only when needed, reducing config verbosity",
      },
      acceptance_criteria: [
        {
          criterion:
            "languages.<host>.bridge changes from array to map with '_' key support",
          verification:
            "Config with languages.markdown.bridge._ = {enabled: true} works; unit tests verify",
        },
        {
          criterion:
            "BridgeLanguageConfig type with 'enabled' (bool) field created",
          verification:
            "languages.markdown.bridge.rust = {enabled: false} disables rust bridging in markdown",
        },
        {
          criterion:
            "Cascade resolution: host.injection > host._ (specific overrides default)",
          verification:
            "Unit tests verify enabled field inherits from '_' when not specified",
        },
        {
          criterion:
            "Backwards compatibility: array format still works via effective_bridge()",
          verification:
            "Old bridge: ['rust', 'python'] config continues to work; deprecation warning logged",
        },
        {
          criterion:
            "Documentation updated with new bridge config syntax",
          verification:
            "README.md shows Before/After examples for bridge config migration",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-124",
      story: {
        role: "Rustacean editing Markdown",
        capability:
          "configure global bridge defaults at languages._ level for all host languages",
        benefit:
          "I can set one default that applies to all host languages without repeating config",
      },
      acceptance_criteria: [
        {
          criterion:
            "Schema supports '_' key in languages map as default for all host languages",
          verification:
            "Config with languages._.bridge._ applies to all host/injection pairs",
        },
        {
          criterion:
            "Four-level cascade: host.injection > host._ > _.injection > _._",
          verification:
            "Unit tests verify correct precedence order",
        },
        {
          criterion:
            "Documentation updated with full cascade examples",
          verification:
            "README.md shows cascade resolution table and examples",
        },
      ],
      status: "draft",
      // Note: Depends on PBI-123; may be combined with it if scope is small
    },
    {
      id: "PBI-125",
      story: {
        role: "Rustacean editing Markdown",
        capability:
          "configure bridge 'mode' to control how injection content is sent to bridge servers",
        benefit:
          "I can optimize bridge behavior for different use cases (isolated vs context-aware)",
      },
      acceptance_criteria: [
        {
          criterion:
            "BridgeLanguageConfig supports 'mode' field with defined semantics",
          verification:
            "mode field accepted in config; semantics documented",
        },
        {
          criterion:
            "E2E tests verify mode behavior differences",
          verification:
            "make test_nvim includes tests demonstrating mode effects",
        },
      ],
      status: "draft",
      // Note: Needs refinement - 'separate' vs 'merged' semantics unclear.
      // Consider: What problem does mode solve? What are the use cases?
      // Options: (1) separate = each injection isolated, (2) merged = all injections combined
    },
  ],

  sprint: {
    number: 101,
    pbi_id: "PBI-121",
    goal: "Extract completion method to lsp_impl/text_document/completion.rs",
    status: "done",
    subtasks: [
      {
        test: "Verify 'make test && make check' passes before refactoring",
        implementation:
          "Baseline verification - ensure all tests pass before starting structural changes",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["336 unit tests + 7 integration tests passed"],
      },
      {
        test: "Verify 'make test && make check' passes after creating completion.rs with module declaration",
        implementation:
          "Create lsp_impl/text_document/completion.rs with empty impl TreeSitterLs block; add mod completion to text_document/mod.rs",
        type: "structural",
        status: "completed",
        commits: [{ hash: "789842a", message: "refactor(lsp): extract semantic_tokens to dedicated module", phase: "green" as CommitPhase }],
        notes: ["Also committed Sprint 100 semantic_tokens extraction in same commit"],
      },
      {
        test: "Verify 'make test && make check' passes after moving completion method",
        implementation:
          "Move completion method from lsp_impl.rs to completion.rs; update imports and re-exports",
        type: "structural",
        status: "completed",
        commits: [{ hash: "dc4ea8c", message: "refactor(lsp): extract completion to dedicated module", phase: "green" as CommitPhase }],
        notes: [],
      },
      {
        test: "Run full test suite: 'make test && make check'",
        implementation:
          "Final verification that all tests pass with no behavioral change; completion logic fully isolated",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["336 unit tests passed, all clippy and fmt checks pass"],
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

  // Historical sprints (recent 2) | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 101,
      pbi_id: "PBI-121",
      goal: "Extract completion method to lsp_impl/text_document/completion.rs",
      status: "done",
      subtasks: [],
    },
    {
      number: 100,
      pbi_id: "PBI-121",
      goal: "Extract semantic_tokens methods to dedicated module (Phase 1)",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 99,
      improvements: [
        {
          action:
            "Documentation sprint was straightforward - clear scope with migration guide Before/After examples made the change easy to understand",
          timing: "immediate",
          status: "completed",
          outcome:
            "PBI-120 completed successfully across 3 sprints (97-99); consistent deprecation pattern documented clearly in README",
        },
        {
          action:
            "effective_parser() and effective_queries() pattern established good precedent for handling backwards-compatible schema changes",
          timing: "immediate",
          status: "completed",
          outcome:
            "Pattern documented: Phase 1 (parser alias) used serde-based implementation, Phase 2 (queries array) used effective_* pattern - both approaches work well for different deprecation scenarios",
        },
        {
          action:
            "Consider splitting multi-phase PBIs at refinement - PBI-120 spanned 3 sprints; defining Phase 1/2/3 as separate PBIs would improve tracking and estimation",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
        {
          action:
            "E2E test naming issue (treesitter_ls vs treesitter-ls) still unresolved - carried forward from Sprint 96/97/98",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
      ],
    },
    {
      sprint: 98,
      improvements: [
        {
          action:
            "Clean TDD implementation with ~20 new tests - effective_queries() pattern elegantly handles both old and new config formats, consistent with effective_parser() from Sprint 97",
          timing: "immediate",
          status: "completed",
          outcome:
            "QueryConfig type with infer_query_kind() using file stem provides simple, predictable API; unified representation simplifies all downstream query consumption code",
        },
        {
          action:
            "Extended existing deprecation warning pattern seamlessly - uses_deprecated_query_fields() and log_deprecation_warnings() extension followed established pattern from Sprint 97",
          timing: "immediate",
          status: "completed",
          outcome:
            "Deprecation detection for highlights/injections/locals fields integrated without any architectural changes; pattern proves reusable across deprecation scenarios",
        },
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
