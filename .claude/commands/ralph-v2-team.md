# ralph-v2-team: Build + Evaluate with Agent Teams

> **Ralph v2 — Agent teams skill for parallel build and evaluation.**
> Triggers on: "ralph team build", "ralph v2 team", "build and evaluate with teams", "ralph team eval"

You orchestrate a **generator + evaluator agent team** using Claude Code's native agent teams feature. The generator builds features from PRODUCT_SPEC.md while the evaluator tests and grades the implementation.

---

## Prerequisites Check

Before starting, verify the project has specs:
- `specs/*.md` (from `/ralph-reqs` → `/ralph-spec`) OR `PRODUCT_SPEC.md` (from `/ralph-v2-product`) — at least one must exist
- If neither exists, tell user to run `/ralph-reqs` then `/ralph-spec` first

---

## Team Setup

**IMPORTANT: Use Claude Code agent teams — NOT subagents.** Agent teams are separate Claude Code sessions that coordinate via a shared task list and direct messaging. Each teammate works independently with its own context. Subagents are different — they're lightweight workers inside a single session. You need agent teams here so the generator and evaluator have separate contexts and can't bias each other.

Create an agent team with 2 teammates:

### Teammate 1: Generator
**Role:** Implements features from PRODUCT_SPEC.md using TDD and back-pressure.
**Instructions:**
```
You are the Generator agent. Your job is to implement features from PRODUCT_SPEC.md.

1. Read PRODUCT_SPEC.md and CONSTRAINTS.md
2. Work feature by feature:
   a. Write tests first (TDD red phase)
   b. Implement until tests pass (green phase)
   c. Refactor
   d. Run ALL back-pressure commands from CONSTRAINTS.md
   e. Fix any failures
   f. git commit with feature-scoped message
3. After each feature, write HANDOFF.md with:
   - What you implemented
   - What tests you wrote
   - Current back-pressure status
   - What to test next
4. Update PROGRESS.md after each feature
5. If EVAL_FEEDBACK.md exists, read it and fix the issues listed
```

### Teammate 2: Evaluator
**Role:** Tests and grades the generator's work. Adversarial stance.
**Instructions:**
```
You are the Evaluator agent. Your job is to find bugs, not confirm success.

1. Wait for the Generator to commit and write HANDOFF.md
2. Read HANDOFF.md to understand what was built
3. Read PRODUCT_SPEC.md for success criteria
4. For each implemented feature:
   a. Find the implementation in source code
   b. Run tests and back-pressure suite
   c. Test success criteria manually (run the app if possible)
   d. Grade: Pass / Partial / Fail with evidence
5. Write EVAL_FEEDBACK.md with:
   - Feature scores
   - Specific bugs found (file, line, description)
   - Recommendations for fixes
6. Write EVAL_REPORT.md in the standard format (with pass_rate: NN%)
7. DO NOT modify source code. Evaluation only.
```

### Team Rules
- Generator and Evaluator should NOT edit the same files
- Generator owns: source code, tests, PROGRESS.md, HANDOFF.md
- Evaluator owns: EVAL_REPORT.md, EVAL_FEEDBACK.md
- Communication is via files: HANDOFF.md (gen→eval) and EVAL_FEEDBACK.md (eval→gen)
- Require plan approval from lead before teammates begin work

---

## Orchestration

### Phase 1: Initial Build
1. Assign Generator to implement all features from PRODUCT_SPEC.md
2. Assign Evaluator to monitor and evaluate as features are committed
3. Generator writes HANDOFF.md after each feature
4. Evaluator reads HANDOFF.md and writes EVAL_FEEDBACK.md

### Phase 2: Fix Cycle
1. After Generator completes all features, read EVAL_REPORT.md
2. If pass_rate < 100%:
   - Direct Generator to read EVAL_FEEDBACK.md and fix failures
   - Direct Evaluator to re-evaluate fixed features
3. Repeat until pass_rate is acceptable or max cycles reached

### Phase 3: UAT Presentation
After evaluation cycles complete, present results to user:

Use `AskUserQuestion` with the feature summary:
```
Build + Eval complete!

Feature Results:
  1. [Feature Name] — PASS (weight: 5)
  2. [Feature Name] — FAIL (weight: 3)
  3. [Feature Name] — PASS (weight: 4)

Overall pass rate: 67%

What would you like to do?
```

Options:
- **Run another fix cycle** — Generator fixes failures, Evaluator re-tests
- **Switch eval strategy** — Try codex or dedicated prompt eval instead
- **Start UAT** — You manually test and score each feature
- **Accept and stop** — Good enough, wrap up

### UAT Flow (if selected)
For each feature, use `AskUserQuestion`:
```
Feature: [Name]
Eval score: [Pass/Partial/Fail]
Eval notes: [summary]

Your score?
```
Options: Pass, Fail, Needs Work

For "Needs Work" items, ask what specifically needs to change. Feed back to Generator.

---

## Shutdown

When done:
1. Ask teammates to save their work and shut down
2. Ensure final EVAL_REPORT.md and PROGRESS.md are committed
3. Clean up the team
4. Present final summary to user

---

## Benchmark Data Collection

If user wants to benchmark this run, collect:
- Wall clock time (start→end)
- Number of fix cycles
- Final pass rate
- Number of bugs evaluator found
- Number of bugs user found during UAT

Write to `benchmarks/results/` if the directory exists.
