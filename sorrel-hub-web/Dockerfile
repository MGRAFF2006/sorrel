FROM node:22-alpine

WORKDIR /app

COPY package.json ./
COPY public ./public
COPY server ./server

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=5180
ENV HUB_API_URL=http://hub:3000

EXPOSE 5180

CMD ["node", "server/dev-server.mjs"]
