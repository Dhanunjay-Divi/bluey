FROM mcr.microsoft.com/playwright:v1.61.1-noble

WORKDIR /app
COPY . .
RUN npm ci \
    && npm run build --workspace @bluey/jobs-automation \
    && npm run build --workspace @bluey/jobs-runner \
    && npm cache clean --force

ENV NODE_ENV=production
ENV PLAYWRIGHT_BROWSERS_PATH=/ms-playwright
EXPOSE 8091
CMD ["node", "runner/dist/server.js"]
