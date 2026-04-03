#!/bin/bash
# v2-codex-review.sh — Codex cross-review eval strategy for Ralph v2
#
# Generates a diff, reads PRODUCT_SPEC.md, and invokes Codex for code review.
# Produces EVAL_REPORT.md in the standard v2 format.
#
# Usage: Called by v2-loop.sh when --eval=codex is set.
#        Can also be run standalone: ./v2/v2-codex-review.sh

set -euo pipefail

# ─── Find last eval tag ─────────────────────────────────────────────

LAST_TAG=$(git tag --sort=-creatordate | grep "^v2-eval-" | head -1 2>/dev/null || echo "")

if [ -z "$LAST_TAG" ]; then
    # No previous eval — diff against initial commit or first commit
    LAST_TAG=$(git rev-list --max-parents=0 HEAD 2>/dev/null | head -1)
fi

echo "Codex cross-review"
echo "  Diff base: $LAST_TAG"

# ─── Generate diff ──────────────────────────────────────────────────

DIFF_FILE=$(mktemp /tmp/ralph-v2-diff-XXXXXX.txt)
git diff "$LAST_TAG"..HEAD -- . ':!*.md' ':!*.json' ':!logs/' > "$DIFF_FILE" 2>/dev/null || \
    git diff HEAD~5..HEAD -- . ':!*.md' ':!*.json' ':!logs/' > "$DIFF_FILE" 2>/dev/null || \
    git diff HEAD -- . ':!*.md' ':!*.json' ':!logs/' > "$DIFF_FILE"

DIFF_SIZE=$(wc -c < "$DIFF_FILE" | tr -d ' ')
echo "  Diff size: ${DIFF_SIZE} bytes"

if [ "$DIFF_SIZE" -lt 10 ]; then
    echo "  No meaningful code changes to review."
    cat > EVAL_REPORT.md <<'EOF'
# Evaluation Report

**Date:** $(date +%Y-%m-%d\ %H:%M)
**Evaluator:** Ralph v2 Codex Cross-Review
**Strategy:** codex

## Summary

pass_rate: 0%
features_total: 0
features_pass: 0
features_partial: 0
features_fail: 0

No code changes detected since last evaluation.
EOF
    rm -f "$DIFF_FILE"
    exit 0
fi

# ─── Build review prompt ────────────────────────────────────────────

PRODUCT_SPEC=""
[ -f "PRODUCT_SPEC.md" ] && PRODUCT_SPEC=$(cat PRODUCT_SPEC.md)

CONSTRAINTS=""
[ -f "CONSTRAINTS.md" ] && CONSTRAINTS=$(cat CONSTRAINTS.md)

REVIEW_PROMPT=$(cat <<PROMPT_EOF
You are performing a code review for a Ralph v2 project. Review the following diff against the product specification.

## Product Specification
${PRODUCT_SPEC:-"No product spec found."}

## Constraints
${CONSTRAINTS:-"No constraints found."}

## Code Diff
$(head -c 50000 "$DIFF_FILE")

## Your Task

1. For each feature in the product spec, check if the diff implements it correctly.
2. Grade each feature: Pass / Partial / Fail with evidence.
3. Check for: bugs, security issues, missing error handling, dead code, stubs/TODOs.
4. Produce your report in this exact format:

# Evaluation Report

**Date:** [today]
**Evaluator:** Ralph v2 Codex Cross-Review
**Strategy:** codex

## Summary

pass_rate: NN%
features_total: N
features_pass: N
features_partial: N
features_fail: N

## Feature Scores

| # | Feature | Weight | Score | Summary |
|---|---------|--------|-------|---------|

## Issues Found

### Critical
### Major
### Minor

## Recommendations
PROMPT_EOF
)

# ─── Try Codex CLI first, fall back to Claude ───────────────────────

if command -v codex &>/dev/null; then
    echo "  Using Codex CLI for review..."
    echo "$REVIEW_PROMPT" | codex -p --model o4-mini 2>/dev/null > EVAL_REPORT.md || {
        echo "  Codex CLI failed, falling back to Claude..."
        echo "$REVIEW_PROMPT" | claude -p --model sonnet --output-format text > EVAL_REPORT.md
    }
else
    echo "  Codex CLI not found, using Claude (Sonnet) as cross-reviewer..."
    echo "$REVIEW_PROMPT" | claude -p --model sonnet --output-format text > EVAL_REPORT.md
fi

# ─── Tag this eval point ────────────────────────────────────────────

EVAL_TAG="v2-eval-codex-$(date +%Y%m%d_%H%M%S)"
git tag "$EVAL_TAG" 2>/dev/null || true

# ─── Cleanup ────────────────────────────────────────────────────────

rm -f "$DIFF_FILE"

echo "  Review complete: EVAL_REPORT.md"
echo "  Tagged: $EVAL_TAG"
