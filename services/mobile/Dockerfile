FROM python:3.12-slim
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
WORKDIR /app
COPY pyproject.toml requirements*.txt ./
COPY zerus_mobile ./zerus_mobile
RUN pip install --no-cache-dir '.[fcm]' && useradd --uid 10001 --create-home relay && mkdir /data && chown relay:relay /data && chmod 700 /data
USER 10001:10001
EXPOSE 8787
ENTRYPOINT ["zerus-mobile", "--database", "/data/relay.sqlite3"]
CMD ["serve", "--host", "0.0.0.0"]
