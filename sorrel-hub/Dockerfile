FROM node:22-alpine

WORKDIR /app

COPY package.json ./
COPY package-lock.json ./
COPY src ./src

ENV NODE_ENV=production
ENV PORT=3000
ENV SORREL_HUB_DATA_DIR=/app/data/sync
ENV SORREL_HUB_METADATA_DIR=/app/data/metadata

EXPOSE 3000

VOLUME ["/app/data"]

CMD ["node", "src/server.js"]
