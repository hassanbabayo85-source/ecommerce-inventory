# Security Policy

## Supported versions

| Version | Supported          |
|---------|--------------------|
| 0.1.x   | :white_check_mark: |

## Reporting a vulnerability

**Please do NOT open a public GitHub issue for security problems.**

Instead, report privately using one of these channels:

1. **GitHub Security Advisories** (preferred):
   <https://github.com/hassanbabayo85-source/ecommerce-inventory/security/advisories/new>
2. **Email**: hassanbabayo85@gmail.com — encrypt sensitive details if possible
   (PGP key on request).

### What to include

- Affected version or commit SHA
- Description of the vulnerability
- Reproduction steps (curl commands, request bodies, etc.)
- Impact assessment (data leak, DoS, privilege escalation, ...)
- Any suggested fix

### What to expect

- **Acknowledgment within 3 business days.**
- **Triage assessment within 7 days.**
- **Coordinated disclosure**: we'll publish a GitHub Security Advisory once a
  fix is available. Credit will be given unless you prefer to remain anonymous.
- **Target fix window**:
  * Critical / High → 14 days
  * Medium → 30 days
  * Low → next regular release

## Scope

In scope:

- This repository's source code, Docker images, and CI workflows.
- The public API surface (`/api/*`, `/health`, `/ready`).

Out of scope:

- Social engineering attacks against maintainers.
- Denial of service via volume (we run behind a reverse proxy with rate limits).
- Vulnerabilities in third-party dependencies — report those upstream, but
  feel free to notify us so we can bump the affected crate.
- Issues in `docker-compose*.yml` that require the attacker to already have
  host-level access.

## Security hardening already in place

- Order ownership enforced via per-order `access_token` (BOLA / IDOR fix).
- CORS strict mode in production (`APP_ENV=production` panics on bad config).
- Admin routes require `X-API-Key` with constant-time comparison.
- Idempotency keys on `POST /api/orders`.
- `SELECT ... FOR UPDATE` row locking; deterministic lock ordering.
- Postgres isolated on `backend` network (internal-only) in production compose.
- Containers: non-root, `read_only` root FS, `no-new-privileges`, resource limits.
- Caddy sets HSTS, X-Content-Type-Options, X-Frame-Options,
  Referrer-Policy, Permissions-Policy.
- Dependency scanning via `cargo audit` in CI.
