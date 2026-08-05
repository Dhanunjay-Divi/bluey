FROM rust:1.95.0-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1 AS native-storage-builder

WORKDIR /build/native-storage
COPY runner/native-storage/Cargo.toml runner/native-storage/Cargo.lock runner/native-storage/build.rs ./
COPY runner/native-storage/src ./src
RUN cargo build --locked --release \
    && test -f target/release/libbluey_jobs_runner_native_storage.so \
    && install -D -m 0555 target/release/libbluey_jobs_runner_native_storage.so \
        /out/bluey_jobs_runner_native_storage.node

FROM mcr.microsoft.com/playwright:v1.61.1-noble@sha256:5b8f294aff9041b7191c34a4bab3ac270157a28774d4b0660e9743297b697e48

WORKDIR /app
COPY . .
COPY --from=native-storage-builder /out/bluey_jobs_runner_native_storage.node \
    /tmp/bluey_jobs_runner_native_storage.node
RUN npm ci \
    && npm run build --workspace @bluey/jobs-automation \
    && npm run build --workspace @bluey/jobs-runner \
    && install -D -m 0555 /tmp/bluey_jobs_runner_native_storage.node \
        /app/runner/dist/native/bluey_jobs_runner_native_storage.node \
    && rm /tmp/bluey_jobs_runner_native_storage.node \
    && npm cache clean --force \
    && install -d -o pwuser -g pwuser -m 0700 /var/lib/bluey-jobs-runner

ENV NODE_ENV=production
ENV PLAYWRIGHT_BROWSERS_PATH=/ms-playwright
ENV BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs-runner
USER pwuser
EXPOSE 8091
CMD ["node", "runner/dist/server.js"]
