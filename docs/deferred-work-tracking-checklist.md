# Deferred Work Tracking Checklist

Created: 2026-01-04 (Sprint 135 Retrospective)
Purpose: Prevent deferred subtasks from becoming forgotten technical debt

## When to Mark Work as Deferred

- Subtask complexity exceeds sprint capacity
- Blocking dependency discovered mid-sprint
- Non-critical acceptance criteria can be postponed without breaking sprint goal
- Feature can deliver value with reduced scope

## Marking Convention

### In Sprint Subtasks

```typescript
{
  test: "Feature description",
  implementation: "DEFERRED: Clear reason why work postponed",
  type: "behavioral",
  status: "pending",  // Keep as pending, not completed
  commits: [],
  notes: [
    "⏸️ DEFERRED: Specific reason",
    "⏸️ Impact: What functionality is missing",
    "⏸️ Follow-up: Reference to follow-up work (PBI ID or next sprint subtask)"
  ]
}
```

### In Acceptance Criteria

```typescript
acceptance_criteria: [
  {
    criterion: "Feature works in basic case",
    verification: "make test passes"
  },
  {
    criterion: "DEFERRED (PBI-XXX): Advanced feature",
    verification: "To be implemented in follow-up PBI"
  }
]
```

## Follow-up PBI Creation Criteria

Create new PBI when deferred work:
- Adds new user-facing capability (timing: product backlog)
- Requires more than 3 subtasks to complete
- Depends on external changes (library updates, architecture decisions)

Add to next sprint when deferred work:
- Completes partially-implemented infrastructure (timing: sprint)
- Required for next sprint's PBI dependencies
- Addresses technical debt blocking future features

Apply immediately when deferred work:
- Process improvement (checklist, guideline documentation)
- Non-production code changes (test helpers, scripts)

## Tracking During Refinement

Before starting new sprint:
1. Review previous sprint retrospectives for deferred work references
2. Check PBI refinement_notes for "DEFERRED" markers
3. Verify follow-up PBIs created or subtasks added to upcoming sprint
4. Update improvement action outcomes in retrospectives

## Examples from Sprint 135

**Deferred Subtask (completed in same sprint):**
```typescript
{
  test: "Completion response ranges translated back to host coordinates",
  implementation: "DEFERRED: Range translation not required for basic completion",
  status: "pending",
  notes: [
    "⏸️ DEFERRED: Pool.completion() currently returns Ok(None)",
    "⏸️ Range translation needed when pool integrates with BridgeConnection",
    "⏸️ translate_virtual_to_host() exists in CacheableInjectionRegion"
  ]
}
```

**Follow-up Sprint Action (from retrospective):**
```typescript
{
  action: "Add Pool-to-BridgeConnection integration subtask to next sprint",
  timing: "sprint",
  status: "active"
}
```

## Anti-Patterns to Avoid

- Marking deferred work as "completed" (breaks DoD integrity)
- Deferring without documenting impact or follow-up plan
- Creating follow-up PBIs without clear acceptance criteria
- Deferring work repeatedly across multiple sprints (indicates scope misalignment)
