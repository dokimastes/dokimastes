# `dok baseline` — the before number

A project can only be compared to itself. The question "did agentic delivery work here" has no defensible answer unless the project's performance was recorded *before* the first agent ran, and that number cannot be reconstructed later. `dok baseline` records the half of it that the repository's own history holds.

```sh
dok baseline --repo path/to/checkout                      # last 180 days
dok baseline --repo . --window-days 90 --format json
dok baseline --repo . --first-agent-run 2026-10-01        # refused if captured after that date
```

## The symmetric contract

Every throughput figure is reported next to the degradation figure the evidence says moves against it, so good news cannot be reported without its counterpart.

| Throughput | Paired counter-metric | From history? |
|---|---|---|
| commits per week | code churn within 14 days | both |
| merges per week | revert commits | both |
| median merge size | incidents per merge | throughput only |
| median lead time | merges with no review | throughput only |
| releases per week | review depth | throughput only |
| — | block duplication and cross-file calls | no |
| — | mutation score | no |
| — | escaped-defect rate per lane | no |

A figure history cannot yield is reported with the reason, in one of three kinds: it lives on the hosting platform (review state, comments), it needs tooling over the code (duplication, mutation), or it did not exist before switch-on and must be collected from now on (incidents, escaped defects). Nothing is left blank.

## How the figures are computed

- **Churn** is lines added in the window and deleted again in the same file within 14 days, at file granularity. That is an upper bound on line-level churn, and the report says so.
- **Merges** are merge commits on the first-parent line. A squash-merged or rebased history has none; the report then says pull-request figures need the platform API rather than showing zeros as if they were measured.
- **Lead time** is the oldest merged commit to the merge commit. **Releases** are `v*` tags in the window, a proxy for deploys.
- Only counts leave the tool. No author, email or login appears in the output; the number of distinct authors is reported as a number.

## The refusal

With `--first-agent-run`, a baseline captured on or after that date is refused with exit code 1. It is not a before number, and a project without one cannot be marked ready.
