ARG BLUEY_JOBS_NODE_IMAGE
ARG SOURCE_DATE_EPOCH
ARG BLUEY_JOBS_BUILD_ID
ARG BLUEY_JOBS_CONFIG_SCHEMA_SHA256
ARG BLUEY_JOBS_MIGRATION_SET_SHA256
ARG BLUEY_JOBS_PROTOCOL_SET_SHA256
ARG BLUEY_JOBS_SOURCE_COMMIT
FROM ${BLUEY_JOBS_NODE_IMAGE} AS build

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
        --workspace @bluey/jobs-workflows
COPY automation/tsconfig.json ./automation/tsconfig.json
COPY automation/src ./automation/src
COPY workflows/tsconfig.json ./workflows/tsconfig.json
COPY workflows/src ./workflows/src
RUN npm run build --workspace @bluey/jobs-automation \
    && npm run build --workspace @bluey/jobs-workflows \
    && npm prune --omit=dev \
        --workspace @bluey/jobs-automation \
        --workspace @bluey/jobs-workflows \
    && find node_modules -type d \
        \( -name src -o -name test -o -name tests -o -name __tests__ \) \
        -prune -exec rm -rf '{}' + \
    && find node_modules -type f \
        \( -name '*.map' -o -name '*.ts' -o -name '*.tsx' \
        -o -name Dockerfile -o -name package-lock.json \) -delete \
    && find node_modules/@bluey -mindepth 1 -maxdepth 1 -type l \
        ! -name jobs-automation ! -name jobs-workflows -delete

ARG BLUEY_JOBS_NODE_IMAGE
FROM ${BLUEY_JOBS_NODE_IMAGE} AS runtime-rootfs

ARG BLUEY_JOBS_BUILD_ID
ARG BLUEY_JOBS_CONFIG_SCHEMA_SHA256
ARG BLUEY_JOBS_MIGRATION_SET_SHA256
ARG BLUEY_JOBS_PROTOCOL_SET_SHA256
ARG BLUEY_JOBS_SOURCE_COMMIT

WORKDIR /app
COPY --from=build --chown=node:node /build/node_modules ./node_modules
COPY --from=build --chown=node:node /build/automation/package.json \
    ./automation/package.json
COPY --from=build --chown=node:node /build/automation/dist ./automation/dist
COPY --from=build --chown=node:node /build/workflows/package.json \
    ./workflows/package.json
COPY --from=build --chown=node:node /build/workflows/dist ./workflows/dist
COPY scripts/managed-cloud-release-gate.mjs \
    /tmp/managed-cloud-release-gate.mjs
RUN node /tmp/managed-cloud-release-gate.mjs runtime-measurement \
        --root / \
        --component jobs-workflows \
        --build-id "$BLUEY_JOBS_BUILD_ID" \
        --config-schema-sha256 "$BLUEY_JOBS_CONFIG_SCHEMA_SHA256" \
        --migration-set-sha256 "$BLUEY_JOBS_MIGRATION_SET_SHA256" \
        --protocol-set-sha256 "$BLUEY_JOBS_PROTOCOL_SET_SHA256" \
        --roles workflow_gateway,workflow_worker \
        --source-commit "$BLUEY_JOBS_SOURCE_COMMIT" \
        --out /app/.bluey/managed-cloud-runtime-measurement.json \
    && rm -rf /usr/local/lib/node_modules /root/.npm /tmp/* \
    && rm -f /usr/local/bin/corepack /usr/local/bin/npm /usr/local/bin/npx \
        /usr/local/bin/yarn /usr/local/bin/yarnpkg /usr/local/bin/docker-entrypoint.sh \
    && find /app -type d -exec chmod 0555 '{}' + \
    && find /app -type f -exec chmod 0444 '{}' +

FROM scratch
COPY --from=runtime-rootfs / /
ENV NODE_ENV=production
ENV PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
WORKDIR /app
ENTRYPOINT []
USER node
CMD ["/usr/local/bin/node", "workflows/dist/worker.js"]
