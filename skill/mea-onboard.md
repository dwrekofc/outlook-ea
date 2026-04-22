---
name: mea-onboard
description: Interactive onboarding — interviews you to set up mea with your contacts, org structure, triage rules, and preferences.
allowed-tools:
  - Read
  - Write
  - Edit
  - Bash
  - Grep
  - Glob
  - AskUserQuestion
user-invocable: true
argument-hint: (no arguments needed)
---

# mea Onboarding Interview

You are setting up a new user's mea (Mail Executive Assistant) installation. Your job is to interview them interactively using AskUserQuestion, learn how they work, and configure mea to match their workflow.

Work through the phases below in order. Be conversational but efficient. After each phase, briefly summarize what you've set up before moving on.

## Pre-flight Check

Before starting the interview, verify the installation:

```bash
which mea          # binary exists
ls ~/.mea/          # data dir exists
mea sync 2>&1      # can talk to Mail.app
```

If anything fails, help the user fix it before proceeding. Refer them to QUICKSTART.md for setup steps.

If `mea sync` succeeds, continue. The database is now initialized.

---

## Phase 1 — Who Are You?

Ask via AskUserQuestion:

**"Let's set up your email assistant. First, tell me about yourself — what's your role, what company/org are you at, and what does a typical day in your inbox look like?"**

Options: (free text — let them type)

From their answer, extract:
- Job title / role
- Company / organization name
- Primary email domain (e.g., `@company.com`)
- General inbox character (high volume? lots of notifications? cross-team comms?)

Record this context — you'll use it to inform graph structure and rule suggestions.

---

## Phase 2 — Org Structure

Ask via AskUserQuestion:

**"Who are the key people in your work life? I want to know about:\n\n1. Your direct manager\n2. Anyone you manage\n3. Close collaborators (people you email daily/weekly)\n4. Key stakeholders (execs, skip-levels, cross-team leads)\n\nList as many as you want — name and role is enough. We can add email addresses later."**

Options: (free text)

For each person mentioned:
- `mea graph add --type person --name "..." --description "..."` 
- If they sound important: `mea graph add-vip --email "..." --name "..." --description "..."`
- Link relationships: `mea graph link --from <id> --to <id> --predicate manages|reports_to|collaborates`

If they mention teams or orgs:
- `mea graph add --type team --name "..."`
- `mea graph add --type org --name "..."`
- Link people to teams: `mea graph link --from <person_id> --to <team_id> --predicate member_of`

Ask follow-up questions if the org structure is unclear. Build out the graph iteratively.

---

## Phase 3 — Inbox Sampling

Now sample their actual inbox to establish patterns:

```bash
mea list --page-size 30
```

Parse the JSON output. Group emails by sender domain. Present a summary via AskUserQuestion:

**"Here's what's in your inbox right now. I've grouped by sender domain:\n\n[domain1] — N emails (senders: ...)\n[domain2] — N emails (senders: ...)\n...\n\nLet's go through these groups. For each one, tell me: is this something you want to keep seeing in your inbox, auto-archive, or auto-trash?"**

Options:
- "Let's go through them"
- "Skip this — I'll triage later"

If they want to go through them, for each domain group, ask via AskUserQuestion:

**"[domain] — N emails from [senders]. Examples:\n- [subject1]\n- [subject2]\n\nWhat should I do with emails from this sender/domain?"**

Options:
- "Keep in inbox" — no rule, these are important
- "Auto-archive" — `mea graph add-rule --name "..." --match-sender "..." --action archive`
- "Auto-trash" — `mea graph add-rule --name "..." --match-sender "..." --action trash`
- "It depends — let me see individual emails"
- "Skip"

If "it depends" — read a few individual emails with `mea read <id>` and ask about each one. Look for subject-line patterns that distinguish important from noise.

For senders they want to keep — check if the person is already in the graph. If not, ask:

**"[sender_name] ([email]) — want me to add them to your contacts graph? Are they a colleague, external vendor, or something else?"**

---

## Phase 4 — Label Preferences

Present the default label system and let them customize:

Ask via AskUserQuestion:

**"mea uses 5 labels to categorize emails. Here are the defaults:\n\n1. Follow Up — needs action from you\n2. Waiting — waiting on someone else\n3. Reference — keep for reference\n4. Read Later — long reads, newsletters\n5. Receipts — purchase confirmations, expenses\n\nDo these work for you, or would you rename/repurpose any?"**

Options:
- "These are fine"
- "I want to change some"

If they want changes, ask which labels to rename and update the skill file:
- Edit `~/.claude/skills/mea/SKILL.md` — update the label descriptions in the Actions section

---

## Phase 5 — Triage Style

Ask via AskUserQuestion:

**"How aggressive should auto-triage be?\n\nConservative: only trash/archive things with explicit rules, ask about everything else\nModerate: trash obvious spam, archive known notifications, ask about the rest\nAggressive: auto-handle anything that matches a pattern, only ask about genuinely new senders"**

Options:
- "Conservative"
- "Moderate"  
- "Aggressive"

Record this preference in PATTERNS.md:

```bash
cat >> ~/.claude/skills/mea/PATTERNS.md << EOF

## $(date +%Y-%m-%d) — Onboarding

- Triage style: [conservative|moderate|aggressive]
EOF
```

---

## Phase 6 — Workflow Preferences

Ask via AskUserQuestion:

**"A few more preferences:\n\n1. Do you want a daily brief each morning? (sync + triage preview + task review)\n2. When triaging, do you prefer batch mode (groups of emails by sender) or one-by-one review?\n3. Should I capture action items from emails as tasks automatically, or only when you ask?\n4. Anything else you want your email assistant to do that I haven't mentioned?"**

Options: (free text)

From their answers:
- Record workflow preferences in PATTERNS.md
- If they want additional features, note them in PATTERNS.md as future requests
- If they describe workflows not covered by the current skill, note those too

---

## Phase 7 — Feature Wishlist

Ask via AskUserQuestion:

**"mea is fully customizable — both the Claude Code skill (how I interact with you) and the Rust CLI (what commands exist). Is there anything you wish an email assistant could do that you've never had? Some ideas others have mentioned:\n\n- Auto-draft replies for common email types\n- Weekly digest summaries\n- Email-to-calendar event detection\n- Sender reputation scoring\n- Thread summarization\n- Custom folder routing rules\n\nAnything sound useful, or do you have your own ideas?"**

Options: (free text)

Record their wishlist in PATTERNS.md under a `## Feature Wishlist` section. These are notes for future sessions — the user or Claude can pick them up later when modifying the skill or CLI.

---

## Phase 8 — Finalize

1. Dump the graph context so it's available for future sessions:
   ```bash
   mea graph dump
   ```

2. Read back `~/.mea/GRAPH_CONTEXT.md` and present a summary of what was set up:
   - Number of people, teams, orgs added
   - Number of triage rules created
   - VIP list
   - Preferences recorded

3. Write a final summary to PATTERNS.md with today's date.

4. Present via AskUserQuestion:

**"Setup complete! Here's what I configured:\n\n[summary]\n\nYou can now use:\n- `/mea daily-brief` — morning inbox review\n- `/mea triage my inbox` — bulk triage session\n- `/mea <anything>` — any email management task\n\nWant to run your first daily brief now, or is there anything you'd like to adjust first?"**

Options:
- "Run daily brief"
- "Let me adjust something first"
- "I'm done for now"

If "Run daily brief" — execute `mea sync` then hand off to the daily brief workflow in SKILL.md.

---

## Guidelines

- Keep questions conversational and concise — don't overwhelm with options
- Build the graph incrementally — it's fine if it's sparse at first, it grows during triage
- Don't force the user through every phase — if they say "skip" or "I'll do this later", respect that
- Always confirm before creating rules that auto-trash (destructive)
- Show what you're doing — after each `mea graph add` or `mea graph add-rule`, briefly confirm what was created
- If the user describes a workflow that doesn't exist in the CLI, note it as a feature request rather than pretending it works
