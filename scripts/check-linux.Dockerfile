# The image `scripts/check-linux.sh` runs the full gate in.
#
# Why this file exists: running `check.sh` in a plain `rust:` image reaches
# eleven of its fourteen steps. The other three need tools the base image does
# not carry, so the local gate could not mirror CI — it died at `wasm bindings`
# with `wasm-pack: command not found` and skipped `api snapshot` for want of
# `cargo-public-api`. A local gate that cannot reproduce the runner is a second
# opinion that agrees with itself.
#
# The honest fix is to provide the tools, not to narrow the claim. Narrowing
# would make the docstring true and the gate weaker, and the gate is the thing
# that caught three CI failures this month that `check.sh` could not see.
#
# Everything here is pinned. An unpinned tool in a reproducibility harness is
# the same defect as an unpinned toolchain in the API snapshot gate: the
# harness drifts, and a failure it reports stops meaning anything specific.

ARG RUST_VERSION=1.99.1
FROM rust:${RUST_VERSION}

# Must match `api-surface`'s `toolchain:` input in .github/workflows/ci.yml and
# PINNED_NIGHTLY in scripts/check-api.sh. rustdoc's JSON format drifts between
# nightlies, so the snapshot gate compares two things that were never
# comparable unless this is the date that generated the committed api/*.txt.
ARG PINNED_NIGHTLY=nightly-2026-06-27
# CI cannot be matched exactly here, and that is a finding rather than a
# shortcoming of this file. `ci.yml` uses `jetli/wasm-pack-action@v0.4.0` with
# no `version:` input, so the runner installs whatever wasm-pack is newest at
# the moment the job runs — `v0.4.0` is the *action's* version, not the tool's.
# 0.15.1 was published 2026-10-30, the same day this image was first built, so
# the `Wasm bindings` and `Examples and demos` jobs changed tool versions under
# a green tick. Pinning the action's input is a one-line ci.yml change and is
# left for the owner; this pins to what the runner installs today so the two
# agree now and the divergence is visible when they stop agreeing.
ARG WASM_PACK_VERSION=0.15.1
# Must match `actions/setup-node`'s node-version in ci.yml.
ARG NODE_MAJOR=20

# `js demos` runs `npm run build:pkg && npm install && npm run typecheck &&
# npm run demo:all`, so node and npm are both required.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
        | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg \
    && echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_${NODE_MAJOR}.x nodistro main" \
        > /etc/apt/sources.list.d/nodesource.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# `clippy (wasm32)` and `wasm bindings` both need the target. Without it
# check.sh fails at the clippy step rather than skipping it.
RUN rustup target add wasm32-unknown-unknown

# The pinned nightly, for `api snapshot`. Deliberately NOT the default: the
# other thirteen steps must run on the stable toolchain this image is named
# for, and `api_snapshot.sh` resolves a nightly itself when the default is not
# one. The default profile, not minimal — `cargo public-api` shells out to
# rustdoc, which minimal does not install.
RUN rustup toolchain install ${PINNED_NIGHTLY}

# `--locked` so the tools' own dependency trees do not drift between builds.
# Note what `--locked` does NOT do: it does not pin the tool's own version,
# which is why both carry an explicit `--version`.
RUN cargo install wasm-pack --version ${WASM_PACK_VERSION} --locked \
    && cargo install cargo-public-api --version 0.52.0 --locked \
    && rm -rf /usr/local/cargo/registry

# The gate mounts the working tree at /work and redirects CARGO_TARGET_DIR
# there, so the container does not fight the host's macOS target/ directory.
WORKDIR /work
