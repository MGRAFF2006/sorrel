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

COPY --from=build /app/sorrel-hub-web/dist ./dist
COPY sorrel-hub-web/server ./server
COPY sorrel-hub-web/package.json ./

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=5180
ENV HUB_API_URL=http://hub:3000

EXPOSE 5180

CMD ["node", "server/static-server.mjs"]
