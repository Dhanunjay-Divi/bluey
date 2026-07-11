FROM node:22-bookworm-slim

WORKDIR /app
COPY . .
RUN npm ci \
    && npm run build --workspace @bluey/jobs-automation \
    && npm run build --workspace @bluey/jobs-workflows \
    && npm prune --omit=dev

ENV NODE_ENV=production
CMD ["node", "workflows/dist/worker.js"]
