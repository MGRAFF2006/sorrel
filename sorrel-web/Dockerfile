FROM nginx:1.27-alpine

COPY index.html styles.css site.js /usr/share/nginx/html/
COPY assets/ /usr/share/nginx/html/assets/
COPY docs/ /usr/share/nginx/html/docs/

EXPOSE 80
