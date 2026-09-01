FROM node:22-alpine AS build

WORKDIR /app

COPY sorrel-hub-ui/package.json sorrel-hub-ui/package-lock.json ./sorrel-hub-ui/
COPY sorrel-hub-web/package.json sorrel-hub-web/package-lock.json ./sorrel-hub-web/

WORKDIR /app/sorrel-hub-ui
RUN npm ci

WORKDIR /app/sorrel-hub-web
RUN npm ci

COPY sorrel-hub-ui /app/sorrel-hub-ui
COPY sorrel-hub-web /app/sorrel-hub-web

WORKDIR /app/sorrel-hub-web
RUN npm run build

FROM node:22-alpine

WORKDIR /app

LABEL org.opencontainers.image.title="Sorrel Hub Web" \
      org.opencontainers.image.description="Browser host and API proxy for Sorrel Hub" \
      org.opencontainers.image.source="https://github.com/MGRAFF2006/sorrel" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0"

COPY --chown=node:node --from=build /app/sorrel-hub-web/dist ./dist
COPY --chown=node:node sorrel-hub-web/server ./server
COPY --chown=node:node sorrel-hub-web/package.json ./

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=5180
ENV HUB_API_URL=http://hub:3000

EXPOSE 5180

USER node

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=5 \
  CMD ["node", "-e", "fetch('http://127.0.0.1:' + (process.env.PORT ?? '5180') + '/').then((response) => { if (!response.ok) process.exit(1); }).catch(() => process.exit(1))"]

CMD ["node", "server/static-server.mjs"]
