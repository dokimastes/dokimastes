# `dok assess` — the substrate verdict

Before an agent runs on a codebase, one question decides everything downstream: can this substrate support agentic delivery at all, and in which mode? `dok assess` answers it with a verdict of green, amber or red, cites the reason for every rating, and names what would have to change. It is the on-ramp: useful to someone who adopts nothing else.

```sh
dok assess --repo path/to/checkout --profile path/to/profile.yaml
dok assess --repo .                       # no profile: every declared fact is unknown
dok assess --repo . --format json
```

## What is measured, what is declared

Some facts can be read from the working tree: which build systems and languages are present, whether the environment is pinned by a container or toolchain file, whether an ownership map exists. Everything else — the test command and whether it is green on `main`, the flake rate, the CI clock, the mutation baseline, who can add a required check — is measured by a person once and recorded in the profile. The report says for every row where the fact came from.

**Unknown is treated as the most restrictive value.** A profile that does not say whether the test suite is green is assessed as if it were not. This is deliberate: anything incomplete starts at the most restrictive treatment or waits, and a verdict built on assumptions is the vendor narrative this framework exists to disprove.

## The verdict and what it permits

| Verdict | Meaning | Mode ceiling |
|---|---|---|
| green | D3 available, widen change classes normally | `m3-session` — or `m3-staged` while the inner loop is at or above five minutes |
| amber | D3 available in `m3-staged` only; restrict change classes | `m3-staged` |
| red | D2 only; the estate work *is* the project | `m2-*` |

Any blocking finding makes the verdict red; any concern makes it amber. The two blocking checks are the test entry point and branch protection: an agent cannot use a suite it cannot invoke, and without required checks settable by someone other than the developers there is no F3 boundary on the project.

D4 (`m4-*`) is never admitted by substrate assessment. Admission to Lane 4 is per workload, by dokimasia, on an independent oracle.

## Refusals

The profile may record a verdict (`substrate`) and a chosen mode (`default_mode`). `dok assess` recomputes the verdict and **refuses** the profile, with exit code 1, when:

- the declared substrate is more permissive than the assessment supports;
- the declared mode exceeds the ceiling the verdict allows;
- the declared mode is D4;
- the declared mode is not one the framework defines.

A refusal is an exit code, not a warning. The mode follows from the substrate, not from preference.

## Thresholds

Three numbers carry the judgement, and they are the design's, from the operating model: flake above 2 % makes the inner loop noise; an inner loop at or above 5 minutes forces `m3-staged`; more than 2 distinct build systems is the fragmentation finding in numbers. They live as named constants at the top of the rules, cited, so a change to one is a reviewable policy change rather than a tweak.

An example profile is in `docs/examples/profile.yaml`.
