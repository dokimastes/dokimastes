# Acceptance scenarios

The acceptance criteria of `dok`, in Gherkin, executed for real: the `dok` binary runs inside a container built from this working tree, against a git repository provisioned in that container. Every feature carries negative scenarios, because happy-path-only acceptance is incomplete by construction.

## The shape of a scenario

| Step | What it does |
|---|---|
| **Given** | provisions: starts a container from the acceptance image, creates a bare repository with `main` and `side` plus a working clone, optionally installs a server-side `pre-receive` hook that refuses pushes, adds files to the tree, writes a profile |
| **When** | runs the real command: `dok conform …` or `dok assess …`, with `--format json` |
| **Then** | reads the command's JSON report and its exit code; for the repository's state, makes one `verify-repo` call and compares it with the state recorded at provisioning |

The `.feature` files are the expectation and live here, under the constitutional path. The step definitions in `tests/bdd.rs` and the container driver in `tests/container/` are the mechanism.

## What they run against

| Feature | Command | Repository |
|---|---|---|
| `conform.feature` | `dok conform --pack /srv/pack.yaml --remote /srv/repo.git --only <probe>` | the bare repository in the container, refusing at push time through a `pre-receive` hook when the scenario says so |
| `assess.feature` | `dok assess --repo /srv/work [--profile /srv/profile.yaml]` | the working clone in the container, with the files the scenario adds |

Nothing touches GitHub, a network, or a credential. The pack that runs against the real repository is `conformance/negative-capability/pack.yaml`; these scenarios are what proves that `dok` behaves as claimed before it is trusted there. The pack the scenarios use is `tests/docker/pack.yaml`; its probe ids are the words the feature files use.

## Running

Needs `docker` or `podman` on the path (`DOK_CONTAINER_RUNTIME` picks one explicitly). The image `dokimastes/bdd:local` is built from `tests/docker/Dockerfile` once per test run; layer caching makes reruns fast.

```sh
cargo test --test bdd            # the scenarios alone
cargo test --all-targets         # everything, scenarios included
```

Without a container runtime the scenarios fail. They do not skip: a claim with no attempt behind it is unproven.

Scenarios name claims, never backlog story ids — the backlog is private, permanently.
