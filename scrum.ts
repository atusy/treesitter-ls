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
      "Enable zero-configuration usage of treesitter-ls: users can start the LSP server and get syntax highlighting for any supported language without manual setup.",
    success_metrics: [
      {
        metric: "Zero-config startup",
        target: "Works with no initializationOptions",
      },
      {
        metric: "Auto language detection",
        target: "Detects language from LSP languageId",
      },
      {
        metric: "Auto parser install",
        target: "Installs missing parsers automatically",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-065
  // For historical details: git log -- scrum.yaml
  product_backlog: [
    // ADR-0005: Language Detection Fallback Chain (Vertical Slices)
    // Each PBI delivers end-to-end user value independently
    // Dependencies: PBI-066 (foundation) -> PBI-067, PBI-068, PBI-070 (parallel)
    {
      id: "PBI-066",
      story: {
        role: "treesitter-ls server",
        capability:
          "check if a Tree-sitter parser is available for a given language name",
        benefit:
          "the fallback chain can continue to the next detection method when a parser is unavailable",
      },
      acceptance_criteria: [
        {
          criterion:
            "has_parser_available(lang) returns true when parser is loaded in registry",
          verification: "cargo test test_has_parser_available_when_loaded",
        },
        {
          criterion:
            "has_parser_available(lang) returns false when parser is not loaded",
          verification: "cargo test test_has_parser_available_when_not_loaded",
        },
        {
          criterion:
            "Method is exposed on LanguageCoordinator for use by detection logic",
          verification: "cargo test test_coordinator_has_parser_available",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-067",
      story: {
        role: "treesitter-ls user",
        capability:
          "have extensionless scripts (e.g., with shebang #!/usr/bin/env python) correctly highlighted",
        benefit:
          "I can work with scripts that have no file extension and still get syntax highlighting",
      },
      // Vertical slice: detection logic + integration into document open flow
      acceptance_criteria: [
        // Detection logic (unit tests)
        {
          criterion: "Shebang '#!/usr/bin/env python' detects 'python'",
          verification: "cargo test test_detect_shebang_python",
        },
        {
          criterion: "Shebang '#!/bin/bash' detects 'bash'",
          verification: "cargo test test_detect_shebang_bash",
        },
        {
          criterion: "Shebang '#!/usr/bin/env node' detects 'javascript'",
          verification: "cargo test test_detect_shebang_node",
        },
        {
          criterion: "Files without shebang return None from shebang detection",
          verification: "cargo test test_detect_shebang_none",
        },
        // Integration into document open (end-to-end value delivery)
        {
          criterion:
            "When languageId is 'plaintext' or missing, shebang detection kicks in",
          verification:
            "cargo test test_shebang_used_when_language_id_plaintext",
        },
        {
          criterion:
            "Shebang detection only runs when languageId parser unavailable (lazy I/O)",
          verification:
            "cargo test test_shebang_skipped_when_language_id_has_parser",
        },
        {
          criterion:
            "E2E: Opening extensionless script with shebang gets semantic tokens",
          verification: "make test_nvim FILE=tests/test_lsp_shebang.lua",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-068",
      story: {
        role: "treesitter-ls user",
        capability:
          "have files with standard extensions get highlighting when languageId is unavailable",
        benefit:
          "I can open any file with a recognized extension and get syntax highlighting as fallback",
      },
      // Vertical slice: extension extraction + integration as fallback after shebang
      acceptance_criteria: [
        // Detection logic (unit tests)
        {
          criterion: "Extension '.rs' returns 'rs' as parser name candidate",
          verification: "cargo test test_detect_extension_rs",
        },
        {
          criterion: "Extension '.py' returns 'py' as parser name candidate",
          verification: "cargo test test_detect_extension_py",
        },
        {
          criterion: "Files without extension return None",
          verification: "cargo test test_detect_extension_none",
        },
        {
          criterion: "Extension is extracted without the leading dot",
          verification: "cargo test test_detect_extension_strips_dot",
        },
        // Integration as fallback in the chain
        {
          criterion: "Extension fallback runs after shebang detection fails",
          verification: "cargo test test_extension_fallback_after_shebang",
        },
        {
          criterion:
            "Full chain: languageId -> shebang -> extension, stopping at first available parser",
          verification: "cargo test test_full_detection_chain",
        },
        {
          criterion:
            "When no method finds available parser, None is returned gracefully",
          verification:
            "cargo test test_detection_chain_returns_none_when_all_fail",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-070",
      story: {
        role: "treesitter-ls user",
        capability:
          "have injected language regions with common aliases (py, js, sh) get syntax highlighting",
        benefit:
          "I can use short language identifiers in any host language (Markdown, HTML, etc.) and still get highlighting",
      },
      // Vertical slice: alias normalization + integration into injection resolution
      acceptance_criteria: [
        // Alias normalization logic (unit tests)
        {
          criterion: "Alias 'py' normalizes to 'python'",
          verification: "cargo test test_normalize_alias_py",
        },
        {
          criterion: "Alias 'js' normalizes to 'javascript'",
          verification: "cargo test test_normalize_alias_js",
        },
        {
          criterion: "Alias 'sh' normalizes to 'bash'",
          verification: "cargo test test_normalize_alias_sh",
        },
        {
          criterion: "Non-alias identifiers pass through unchanged",
          verification: "cargo test test_normalize_alias_passthrough",
        },
        // Integration into injection resolution
        {
          criterion:
            "Direct identifier is tried first before alias normalization",
          verification: "cargo test test_injection_direct_identifier_first",
        },
        {
          criterion:
            "Injection resolution uses alias normalization when direct lookup fails",
          verification: "cargo test test_injection_uses_alias_normalization",
        },
        {
          criterion:
            "Unknown aliases with no available parser return None (graceful degradation)",
          verification: "cargo test test_injection_unknown_alias_returns_none",
        },
        {
          criterion:
            "E2E: Markdown with ```py code fence gets Python semantic tokens",
          verification:
            "make test_nvim FILE=tests/test_lsp_injection_alias.lua",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 49,
    pbi_id: "PBI-066",
    goal: "Enable parser availability checks so the fallback chain can determine when to continue to the next detection method",
    status: "in_progress",
    subtasks: [
      {
        test: "has_parser_available returns true when parser is registered in LanguageRegistry",
        implementation: "Add has_parser_available method to LanguageRegistry that delegates to contains()",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: ["Foundation for AC-1: registry-level check"],
      },
      {
        test: "has_parser_available returns false when parser is not registered",
        implementation: "Verify false case - no new code needed, just test coverage",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: ["Covers AC-2: negative case verification"],
      },
      {
        test: "LanguageCoordinator.has_parser_available delegates to registry method",
        implementation: "Add has_parser_available method to LanguageCoordinator that delegates to language_registry",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: ["Covers AC-3: coordinator exposes the API for detection logic"],
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
  // Sprint 1-45 details: git log -- scrum.yaml
  completed: [
    {
      number: 48,
      pbi_id: "PBI-061",
      goal: "Remove filetypes field - language detection via languageId",
      status: "done",
      subtasks: [],
    },
    {
      number: 47,
      pbi_id: "PBI-064",
      goal: "Add injections field for custom injection query paths",
      status: "done",
      subtasks: [],
    },
    {
      number: 46,
      pbi_id: "PBI-065",
      goal: "Update dependencies: tree-sitter 0.26.3, tokio 1.48.0",
      status: "done",
      subtasks: [],
    },
  ],

  retrospectives: [],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
