# ralph-quickie: Ad-hoc Build + Eval + Document Team

> `/ralph-quickie [task]` — any project, Ralph or not.
> Triggers: "ralph quickie", "quickie build", "quick build and eval"
> Zero dependency on external files. Self-contained workflow.

3-agent team: Generator (builds), Evaluator (quality gate), Documentor (traces everything). You are the lead orchestrating all three.

---

## Phase 0: Understand the Project

Before any work, deeply explore the project. Read source code, configs, docs, tests, build files, recent git history, directory structure. Understand what the project is, how it's built, how it's tested, what conventions it follows. Use as many parallel subagents as needed to do this fast.

Skip irrelevant dirs (node_modules, .git, build artifacts, vendored deps). Focus on human-authored code and config.

**You must understand the project well enough to answer:**
- What does this project do?
- What language/framework/tools does it use?
- How is it built? How is it tested? How is it linted?
- What conventions does the codebase follow?
- What is the current state — working? broken? partial?

---

## Phase 1: Clarify (only if uncertain)

Read the user's task. Rate your confidence on three things:
- **What** — the deliverable. What are we building or changing?
- **Why** — the goal. What problem does this solve or what value does it create?
- **Scope** — what's in, what's out?

All ≥90% → skip to Phase 2. Any <90% → ask the user via `AskUserQuestion`. Max 5 questions. Only ask about what and why, never how. Batch into one call.

---

## Phase 2: Set Up Back-Pressure

**What back-pressure is:** Automated feedback that tells the agent whether its work is correct without human involvement. Every change triggers automated checks that report pass/fail. This keeps the agent aligned over long tasks and catches mistakes immediately.

**Why it matters:** Without back-pressure, the agent relies on the user to spot every mistake. That doesn't scale. With back-pressure, the agent self-corrects. More automated feedback = longer tasks you can safely delegate.

**Types of back-pressure (set up ALL that apply):**
1. **Build verification** — does the project compile/bundle without errors?
2. **Automated tests** — do existing tests pass? Can we write new ones for new work?
3. **Static analysis / linting** — does the code follow rules and catch common bugs?
4. **Type checking** — does the type system catch errors? (typed languages only)
5. **Format checking** — does the code match the project's formatting standard?
6. **Runtime verification** — can we start the app/service and verify it responds correctly?
7. **Spec/contract verification** — does the output match a defined schema, API contract, or acceptance criteria?
8. **Simulated user acceptance testing** — actually run the app/CLI/service and use it as a real user would. For web apps: launch a browser (Playwright or similar), navigate pages, click buttons, fill forms, screenshot results. For CLIs: run real commands with real inputs and verify outputs. For APIs: make actual HTTP requests. Test happy paths AND edge cases. This is the most valuable form of back-pressure because it catches issues that pass every other check but still fail in practice.

**What success looks like:** Before ANY code is committed, every applicable back-pressure check runs and passes. If a check fails, the agent fixes it before moving on. The user should never see a commit that breaks the build or fails tests.

Discover what back-pressure is available by examining the project's build files, test configs, CI pipelines, and documentation. If the project has no tests or linting, SET THEM UP as part of the work — back-pressure is not optional.

---

## Phase 3: Launch Team

Create 3 teammates. Give each their role, the task context from Phase 0, and the back-pressure commands from Phase 2.

### Generator
Executes the task. Builds the thing.

**What:** Implement the user's task by writing code, tests, and configuration.
**Why:** This is the primary output — working, tested, complete code.
**Success looks like:**
- Code follows existing project conventions
- Every change has automated tests proving it works
- All back-pressure passes before every commit
- Commits are focused — one logical change per commit
- No stubs, no TODOs, no placeholders — everything fully implemented
- If previous eval feedback exists (EVAL_FEEDBACK.md), those issues are fixed first

After each commit, update a HANDOFF_STATUS.md file with: what changed, what to test next. This is how the Evaluator knows what to check.

### Evaluator
Finds bugs. Simulates user testing. Provides the adversarial quality gate.

**What:** Test everything the Generator builds. Run back-pressure. Try to break it. Report findings.
**Why:** Agents are biased toward their own work — they praise mediocre output and miss bugs. A separate evaluator with fresh eyes catches what the generator won't. This is the highest-leverage improvement to agent quality.
**Success looks like:**
- Every claimed behavior is tested against actual behavior
- Edge cases and error paths are tested, not just happy paths
- User workflows are simulated end-to-end (not just unit-level checks)
- Findings are specific: file, line, reproduction steps — not vague descriptions
- Back-pressure results are recorded
- A clear PASS/FAIL/PARTIAL verdict per feature with evidence
- Evaluator NEVER modifies source code — evaluation only

Write findings to EVAL_FEEDBACK.md so the Generator can read and fix them.

### Documentor
Observes and records everything the team does.

**What:** Maintain two documents:

**HANDOFF.md — The HOW (detailed work trace)**
Chronological record of every significant action: what was built, what decisions were made, what bugs were found and how they were fixed, what back-pressure failures occurred and how they were resolved, commit references, gotchas and workarounds. A new collaborator or agent should be able to read this and recreate all the work.

**MANIFEST.md — The WHAT & WHY (project summary)**
Single-page overview: what the project is, why it exists, what was built/changed in this session, current state, how to build/test/run, key decisions. Hard limit: under 200 lines, under 1000 words. A new collaborator should understand everything in a 2-minute read.

**Why:** Without docs, knowledge dies when the session ends. These files make work reproducible and transferable.
**Success looks like:**
- HANDOFF.md updated after every commit and eval cycle
- MANIFEST.md reflects actual project state (written at start and end)
- Documentor never modifies source code — docs only

### Team Coordination
- Each teammate writes only to their own files. No cross-edits.
- Communication happens through the handoff files each teammate produces.
- Lead (you) monitors progress and redirects if anyone gets stuck.
- Require plan approval from lead before teammates begin work.

---

## Phase 4: Run

Assign all 3 teammates with full context. Flow:
1. Generator builds → Evaluator tests → Documentor tracks
2. Eval failures → Generator fixes → Evaluator re-tests
3. Repeat until: all back-pressure passes AND evaluator finds no critical issues — OR — user decides to stop

---

## Phase 5: Done

Present results to user via `AskUserQuestion`:

> Results: [eval summary]. Docs: HANDOFF.md (N lines), MANIFEST.md (N lines)

Options: **Accept** | **Another cycle** | **Manual review** | **Discard**

Commit final docs on accept. Shut down team.
