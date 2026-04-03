# CLI Interface

## Source
JTBD 1-6 (foundational — the surface through which all jobs are accessed)

## Topic Statement
The system exposes all functionality through a CLI binary (`mea`) that outputs JSON for consumption by the Claude Code skill wrapper.

## Scope
**In-scope:** Command structure, argument patterns, JSON output format, error reporting, --yes flag for destructive ops, exit codes
**Boundaries:** What each command does internally is owned by the respective topic spec. The skill wrapper that calls these commands is owned by skill-wrapper.

## Data Contracts
- SuccessResponse: { status: "ok", data: object }
- ErrorResponse: { status: "error", error: string, code: string }
- ConfirmationResponse: { status: "confirm", message: string, action: string, count: int }

## Behaviors (execution order)
1. On any command: parse arguments, execute, return JSON to stdout
2. On success: return SuccessResponse with relevant data
3. On error: return ErrorResponse with human-readable error and machine-readable code
4. On destructive action without --yes: return ConfirmationResponse describing what would happen
5. On destructive action with --yes: execute and return SuccessResponse
6. All output goes to stdout as JSON — no stderr for normal operation, no interactive prompts

## Constraints
- Rust (edition 2024), binary named `mea`
- Agent-facing: designed for AI agent consumption, not human interactive use
- No REPL, no interactive prompts, no TUI
- All output is machine-parseable JSON
- Exit code 0 for success, non-zero for errors
- Single user, single account

## Acceptance Criteria
1. Binary is named `mea` and runs from command line
2. All commands output valid JSON to stdout
3. Errors return structured JSON with error code and message
4. Destructive commands without --yes return a confirmation prompt (not executed)
5. Destructive commands with --yes execute immediately
6. Exit codes distinguish success from failure
7. No interactive prompts or user input required during execution

## References
- Related: skill-wrapper (calls mea commands)
- Related: mail-actions (--yes flag for destructive ops)
