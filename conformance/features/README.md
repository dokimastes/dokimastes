# Acceptance scenarios

The acceptance criteria of `dok`, in Gherkin. Each scenario is a claim in the form *given this state, when dok does this, then this is what happens* — and every feature carries at least one negative scenario, because happy-path-only acceptance is incomplete by construction.

## What they run against

Nothing outside this repository. The scenarios execute in-process against the `dok` library through `tests/bdd.rs`:

| Feature | Subject | Substrate |
|---|---|---|
| `assess.feature` | `dok assess` rules | synthetic profiles and measured trees built in the steps |
| `conform.feature` | `dok conform` attempts and verdicts | a local bare git repository created per scenario, with or without a `pre-receive` hook that refuses with ruleset wording |

No scenario touches GitHub, a network, or a credential. The pack that runs against the real repository is `conformance/negative-capability/pack.yaml`, executed by `dok conform`; these scenarios are what proves that `dok conform` itself behaves as claimed before it is trusted to run there.

## Where the judgement sits

The `.feature` files are the expectation; the step definitions in `tests/bdd.rs` are the mechanism. A change to a feature file changes what counts as correct, which is why the features live under `conformance/`, the constitutional path, and not under `tests/`.

Scenarios name claims, never backlog story ids — the backlog is private, permanently.

## Running

```sh
cargo test --test bdd            # the scenarios alone
cargo test --all-targets         # everything, scenarios included
```
