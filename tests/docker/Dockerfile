# The image the acceptance scenarios run in: the real `dok` binary, git,
# and the scripts that provision and verify a repository. Built once per
# test run from the working tree, so what is tested is what is checked in.
FROM rust:1.98.0-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
# Dependencies first, so a source change does not rebuild them.
RUN mkdir src tests && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs && echo 'fn main() {}' > tests/bdd.rs \
 && cargo build --release --bin dok \
 && rm -rf src target/release/dok target/release/deps/dok-* target/release/deps/libdok-*
COPY src ./src
RUN cargo build --release --bin dok

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/dok /usr/local/bin/dok
COPY tests/docker/provision-repo tests/docker/provision-history tests/docker/verify-repo tests/docker/refuse-pushes /usr/local/bin/
COPY tests/docker/pack.yaml /srv/pack.yaml
RUN chmod +x /usr/local/bin/provision-repo /usr/local/bin/provision-history /usr/local/bin/verify-repo /usr/local/bin/refuse-pushes
CMD ["sleep", "infinity"]
