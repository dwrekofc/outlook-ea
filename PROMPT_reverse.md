<!-- description: Reverse-engineer existing code into implementation-free specs -->
0a. Study `specs/*` with up to 250 parallel Sonnet subagents to learn existing specifications.
0b. Study `src/*` to understand the codebase. Use up to 500 parallel Sonnet subagents for reads/searches. Treat `src/lib` as the project's standard library for shared utilities and components.

1. For each topic assigned (or discovered), reverse-engineer the source code and produce a specification in `specs/`. Use Opus subagents for complex tracing. Ultrathink. Before writing a spec, search to confirm one doesn't already exist for that topic.
2. One topic per spec. Must pass the "one sentence without 'and'" test. Split if "and" joins unrelated capabilities.
3. **Two-phase process:** Phase 1 (Investigation) — trace every entry point, branch, code path to terminal. Map data flow, side effects, state mutations, error handling, concurrency, config-driven paths, implicit behavior. Phase 2 (Output) — zero implementation details. No function/class/variable names, file paths, library/framework references. A different team on a different stack must be able to reimplement from the spec alone.
4. **Document reality, not intent.** Bugs are features. Never add behaviors the code doesn't implement. Never suggest improvements. If a source comment contradicts the code, document the code's behavior and ignore the comment.
5. **Scope boundaries:** When tracing leaves the topic, stop. Document what crosses the boundary (sent/received) only. Test: "Could this change without changing my topic's outcomes?" If yes, it's across the boundary.
6. **Shared behavior:** Inline fully in every spec (self-contained). Note shared topics for cross-spec tracking. Shared behavior also gets its own canonical spec.
7. **Spec format:** Use the template below for every spec file. File naming: `specs/NN-kebab-case.md` (e.g., `01-session-management.md`).

---

## Spec File Template

Every spec file in `specs/` must use this structure:

```markdown
# Topic Name

## Topic Statement
The system [does what] [for whom/what purpose]. One sentence.

## Scope
**In-scope:** [comma-separated concerns this spec covers]
**Boundaries:** [what is explicitly out of scope and which topic owns it]

## Data Contracts
Concrete data shapes that other topics depend on or that define an API contract.
- Entity name: { field: type, field: type, ... }
- Response shape: { field: type, ... }
Use descriptive types (string, int, ISO-8601 timestamp), not language-specific types.
Omit this section if all data shapes are internal to the topic.

## Behaviors (execution order)
Numbered list describing what the system does, in the order it happens.
1. On [trigger]: [what happens]
2. On [trigger]: [what happens]

## State Transitions
Lifecycle of the primary entity in this topic. Omit for topics without stateful entities.
- State A → State B (trigger)
- [notable] Any non-obvious transition behavior

## Cross-Topic Shared Behavior
Shared concerns that apply to this topic from other specs.
- [Shared concern] applies to [which operations] (see [other-topic] spec)

## Constraints
Bulleted list of rules, boundaries, and locked decisions.
- Where it lives (crate, module, directory)
- What it must NOT depend on
- Patterns to follow
- Performance, compatibility, or platform requirements

## Acceptance Criteria
Numbered list of concrete checks. Each is a true/false statement when examining the codebase.
1. Specific behavior is observable
2. Specific files/modules exist
3. Integration points with other topics work

## References
Optional. Pointers to reference implementations, related specs, or planning doc sections.
- Related: [other-spec] ([boundary description])
- Reference: [URL or file path]
```

---

8. When specs are complete and validated, `git add` all the specs updated/created then `git commit` with a message describing which specs were added/updated.

99999. **Exhaustive checklist before finalizing:** Every entry point documented. Every branch traced to terminal. Every data contract. Every side effect in execution order. Every error path (caught/propagated/ignored). Every config-driven path. Concurrency outcomes. Unreachable paths marked. Notable/surprising behavior marked. Zero implementation details in output. If any item is missing, trace again.
999999. The code is the source of truth. If specs are inconsistent with the code, update the spec using an Opus 4.6 subagent.
9999999. Single sources of truth, no duplicated specs. Update existing specs rather than creating new ones.
99999999. When you learn something new about the project, update @AGENTS.md using a subagent but keep it brief and operational only — no status updates or progress notes.
999999999. Source comments explaining why behavior must be preserved (regulatory, compatibility, intentional) — capture rationale, strip implementation references. Stale comments are not spec.
9999999999. Document all configuration-driven paths, not just the currently active one.
99999999999. If you find inconsistencies in `specs/*` then use an Opus 4.6 subagent with 'ultrathink' to update the specs.
999999999999. This prompt runs AUTONOMOUSLY in a headless loop. Do NOT use AskUserQuestion or any interactive tools. Do NOT stop and wait for user input. Complete the full cycle every iteration.
