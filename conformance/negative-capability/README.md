# Negative-capability suite

The set of things the framework claims an agent, a maintainer or a repository admin **cannot** do to this repository, each tried for real. It is the difference between enforcement and documentation: every row in `pack.yaml` is a sentence in a pitch, an audit response or a Betriebsrat agreement, and without an attempt behind it that is all it is.

## The discipline

1. **Red first.** Run the pack against the repository *before* protection is applied and confirm every attempt succeeds. A probe refused before protection was applied is a failed rehearsal, not a pass: something other than the control under test fired, and that something will not be there next time.
2. **Then green.** Apply the protection and run again. Every attempt must be refused and every assertion must hold.
3. **Name the mechanism.** "Blocked" and "blocked by the control you think" are different results. A refusal whose wording matches none of the mechanisms the pack names is reported as a finding, not quietly counted.
4. **Not-run is not pass.** A probe that needs an identity the runner does not hold, or an action this release cannot perform, is reported as *unproven* and fails the run. Narrowing the probe set because a probe looked hard is the failure this project exists to prevent.

## Identities

A refusal only demonstrates something when the identity had no standing bypass. An organisation owner being allowed to force-push `main` proves nothing about the ruleset. Each probe therefore lists the identities it is meaningful for, and `dok conform --as <identity>` refuses to count a probe under any other.

| `--as` | Meaning |
|---|---|
| `agent` | the agent-class token: push to `agent/**`, nothing else |
| `maintainer` | `@dokimastes/maintainers`; no ruleset bypass |
| `steward` | `@dokimastes/stewards`; on the bypass list, audited |
| `repo-admin` | repository admin who is **not** an organisation owner |

Probe NC-09 — a repository admin removing a required check — is the one that matters most. It is the entire claim organisation-level rulesets exist to make, and it is untestable from an owner account.

## Running it

```sh
# before protection, from a maintainer credential, pushing over SSH
dok conform --pack conformance/negative-capability/pack.yaml --as maintainer --expect red \
  --remote git@github.com:dokimastes/dokimastes.git

# after protection
dok conform --pack conformance/negative-capability/pack.yaml --as maintainer --expect green \
  --remote git@github.com:dokimastes/dokimastes.git

# see what would run without touching anything
dok conform --pack conformance/negative-capability/pack.yaml --as agent --expect green --dry-run
```

Destructive attempts that go through are undone immediately: a force-push is reverted to the previous commit, a deleted branch is recreated, a pushed tag is deleted. The report says whether that worked. If restoration failed, the report shouts, and the repair is manual.

## What this release cannot do yet

NC-07, NC-08 and NC-09 need either a pull request flow or a second identity. `dok` reports them as not-run with the reason. Script them, run them by hand, record the result. Do not delete them from the pack.

## Response wording

Patterns are substrings of the platform's response. Wording marked *recorded* in `pack.yaml` was observed on a real refusal. Wording marked *expected* is best knowledge and must be corrected from the first real refusal. A pattern that never matches is not a bug in the platform; it is a guess in the pack.
