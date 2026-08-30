ARG SOURCE_DATE_EPOCH
ARG BLUEY_JOBS_BUILD_ID
ARG BLUEY_JOBS_CONFIG_SCHEMA_SHA256
ARG BLUEY_JOBS_MIGRATION_SET_SHA256
ARG BLUEY_JOBS_PROTOCOL_SET_SHA256
ARG BLUEY_JOBS_SOURCE_COMMIT
FROM rust:1.95.0-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1 AS native-storage-builder

WORKDIR /build/native-storage
COPY runner/native-storage/Cargo.toml runner/native-storage/Cargo.lock \
    runner/native-storage/build.rs ./
COPY runner/native-storage/src ./src
RUN cargo build --locked --release \
    && install -D -m 0555 target/release/libbluey_jobs_runner_native_storage.so \
        /out/bluey_jobs_runner_native_storage.node

FROM mcr.microsoft.com/playwright:v1.61.1-noble@sha256:5b8f294aff9041b7191c34a4bab3ac270157a28774d4b0660e9743297b697e48 AS build

ENV PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
WORKDIR /build
COPY package.json package-lock.json ./
COPY automation/package.json ./automation/package.json
COPY browser/package.json ./browser/package.json
COPY portal/package.json ./portal/package.json
COPY runner/package.json ./runner/package.json
COPY workflows/package.json ./workflows/package.json
RUN npm ci --include-workspace-root \
        --workspace @bluey/jobs-automation \
        --workspace @bluey/jobs-runner
COPY automation/tsconfig.json ./automation/tsconfig.json
COPY automation/src ./automation/src
COPY runner/tsconfig.json ./runner/tsconfig.json
COPY runner/src ./runner/src
COPY --from=native-storage-builder /out/bluey_jobs_runner_native_storage.node \
    /tmp/bluey_jobs_runner_native_storage.node
RUN npm run build --workspace @bluey/jobs-automation \
    && npm run build --workspace @bluey/jobs-runner \
    && install -D -m 0555 /tmp/bluey_jobs_runner_native_storage.node \
        /build/runner/dist/native/bluey_jobs_runner_native_storage.node \
    && npm prune --omit=dev \
        --workspace @bluey/jobs-automation \
        --workspace @bluey/jobs-runner \
    && find node_modules -type d \
        \( -name src -o -name test -o -name tests -o -name __tests__ \) \
        -prune -exec rm -rf '{}' + \
    && find node_modules -type f \
        \( -name '*.map' -o -name '*.ts' -o -name '*.tsx' \
        -o -name Dockerfile -o -name package-lock.json \) -delete \
    && find node_modules/@bluey -mindepth 1 -maxdepth 1 -type l \
        ! -name jobs-automation ! -name jobs-runner -delete

FROM mcr.microsoft.com/playwright:v1.61.1-noble@sha256:5b8f294aff9041b7191c34a4bab3ac270157a28774d4b0660e9743297b697e48 AS runtime-rootfs

ARG BLUEY_JOBS_BUILD_ID
ARG BLUEY_JOBS_CONFIG_SCHEMA_SHA256
ARG BLUEY_JOBS_MIGRATION_SET_SHA256
ARG BLUEY_JOBS_PROTOCOL_SET_SHA256
ARG BLUEY_JOBS_SOURCE_COMMIT

WORKDIR /app
COPY --from=build --chown=pwuser:pwuser /build/node_modules ./node_modules
COPY --from=build --chown=pwuser:pwuser /build/automation/package.json \
    ./automation/package.json
COPY --from=build --chown=pwuser:pwuser /build/automation/dist ./automation/dist
COPY --from=build --chown=pwuser:pwuser /build/runner/package.json \
    ./runner/package.json
COPY --from=build --chown=pwuser:pwuser /build/runner/dist ./runner/dist
COPY scripts/managed-cloud-release-gate.mjs \
    /tmp/managed-cloud-release-gate.mjs
RUN test "$(command -v node)" = /usr/bin/node \
    && install -m 0555 /usr/bin/node /usr/local/bin/node \
    && test -f /usr/local/bin/node \
    && ! test -L /usr/local/bin/node \
    && find /ms-playwright -mindepth 1 -maxdepth 1 \
        ! -name 'chromium_headless_shell-*' \
        ! -name 'ffmpeg-*' -exec rm -rf '{}' + \
    && /usr/local/bin/node /tmp/managed-cloud-release-gate.mjs runtime-measurement \
        --root / \
        --component jobs-runner \
        --build-id "$BLUEY_JOBS_BUILD_ID" \
        --config-schema-sha256 "$BLUEY_JOBS_CONFIG_SCHEMA_SHA256" \
        --migration-set-sha256 "$BLUEY_JOBS_MIGRATION_SET_SHA256" \
        --protocol-set-sha256 "$BLUEY_JOBS_PROTOCOL_SET_SHA256" \
        --roles managed_runner \
        --source-commit "$BLUEY_JOBS_SOURCE_COMMIT" \
        --out /app/.bluey/managed-cloud-runtime-measurement.json \
    && rm -rf /usr/lib/node_modules /usr/local/lib/node_modules /root/.npm /tmp/* \
    && rm -f /usr/bin/corepack /usr/bin/npm /usr/bin/npx \
        /usr/bin/node /usr/bin/yarn /usr/bin/yarnpkg \
        /usr/local/bin/corepack /usr/local/bin/npm /usr/local/bin/npx \
        /usr/local/bin/yarn /usr/local/bin/yarnpkg \
        /usr/local/bin/docker-entrypoint.sh \
    && find /app -type d -exec chmod 0555 '{}' + \
    && find /app -type f -exec chmod 0444 '{}' + \
    && chmod 0555 /app/runner/dist/native/bluey_jobs_runner_native_storage.node \
    && install -d -o pwuser -g pwuser -m 0700 /var/lib/bluey-jobs-runner

FROM scratch
COPY --from=runtime-rootfs / /
ENV BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs-runner
ENV NODE_ENV=production
ENV PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
ENV PLAYWRIGHT_BROWSERS_PATH=/ms-playwright
WORKDIR /app
ENTRYPOINT []
USER pwuser
EXPOSE 8091
CMD ["/usr/local/bin/node", "runner/dist/server.js"]
