# Production Deployment

## Prerequisites

- A Linux host with Docker Engine 24+ and Docker Compose v2.
- A public DNS **A record** pointing `$DOMAIN` at the host IP
  (needed for Let's Encrypt TLS).
- Ports 80 and 443 open in the firewall.

## First deploy

1. Clone the repo on the host:

       git clone https://github.com/hassanbabayo85-source/ecommerce-inventory.git
       cd ecommerce-inventory

2. Create the production env file:

       cp .env.prod.example .env.prod

3. Generate strong secrets (replace CHANGE_ME_* in .env.prod):

       openssl rand -hex 32   # POSTGRES_PASSWORD
       openssl rand -hex 32   # ADMIN_API_KEY

4. Edit .env.prod: set DOMAIN, ACME_EMAIL, CORS_ALLOWED_ORIGINS.

5. Build and start everything:

       docker compose -f docker-compose.prod.yml --env-file .env.prod up -d --build

6. Watch startup logs:

       docker compose -f docker-compose.prod.yml --env-file .env.prod logs -f

First request to https://$DOMAIN/health triggers TLS provisioning
(usually ~10s after Caddy starts).

## Subsequent deploys

    git pull
    docker compose -f docker-compose.prod.yml --env-file .env.prod up -d --build

## Backups

Postgres data lives in the named volume `ecommerce_pgdata`. Back up:

    docker compose -f docker-compose.prod.yml exec -T postgres \
      pg_dump -U "$POSTGRES_USER" "$POSTGRES_DB" \
      | gzip > "backup-$(date +%F-%H%M).sql.gz"

Restore:

    gunzip -c backup-2026-11-06-1200.sql.gz \
      | docker compose -f docker-compose.prod.yml exec -T postgres \
          psql -U "$POSTGRES_USER" "$POSTGRES_DB"

## Security posture

- PostgreSQL is on the `backend` network only (`internal: true`) —
  never exposed to the host or internet.
- App container runs as non-root (uid 10001), `read_only` root FS,
  `no-new-privileges`, 256MB memory limit.
- Caddy handles TLS + sets HSTS, X-Content-Type-Options, X-Frame-Options,
  Referrer-Policy, Permissions-Policy.
- App requires `APP_ENV=production` → CORS strict mode: startup panics
  if `CORS_ALLOWED_ORIGINS` is empty or contains `*`.

## Health endpoints

- `GET /health` — liveness (process alive).
- `GET /ready`  — readiness (DB reachable).
