FROM python:3.12-slim
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1 TMPDIR=/spool
WORKDIR /app
COPY pyproject.toml requirements*.txt ./
COPY zerus_mobile ./zerus_mobile
RUN pip install --no-cache-dir '.[fcm]' && useradd --uid 10001 --create-home relay && mkdir /data /spool && chown relay:relay /data /spool && chmod 700 /data /spool
USER 10001:10001
EXPOSE 8787
ENTRYPOINT ["zerus-mobile", "--database", "/data/relay.sqlite3"]
CMD ["serve", "--host", "0.0.0.0"]
