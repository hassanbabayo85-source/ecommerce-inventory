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


## Known transitive advisories (accepted risk)

`cargo audit` reports the following RUSTSEC advisories against transitive
dependencies. Each is explicitly ignored in `.cargo/audit.toml` with a
written justification. This is the standard practice recommended by the
RustSec project for advisories that do not apply to a given deployment.

| Advisory | Crate | Impact | Why accepted |
|----------|-------|--------|--------------|
| RUSTSEC-2023-0071 | `rsa 0.9.10` | Marvin Attack (RSA decryption timing) | No upstream fix. We perform no RSA decryption; the crate is a transitive dev-only dependency. |
| RUSTSEC-2026-0258 | `h2 0.3.x` | Unbounded empty DATA frames (DoS) | `h2` is pulled by `reqwest 0.11`, a **dev-dependency** only. Not in the release binary. Fixed by upgrading reqwest to 0.12. |
| RUSTSEC-2026-0349 | `rustls 0.21.x` | Downgrade attack on TLS handshake | Requires active MITM. In production PostgreSQL is on the isolated `backend` Docker network (`internal: true`). Fixed by sqlx 0.8. |
| RUSTSEC-2026-0346 | `rustls 0.21.x` | Debug impl exposes TLS secrets | We never Debug-print TLS session values. Fixed by sqlx 0.8. |
| RUSTSEC-2026-0104 | `rustls-webpki 0.101.x` | Panic in CRL parsing | We do not use CRLs. Fixed by sqlx 0.8. |
| RUSTSEC-2026-0098 | `rustls-webpki 0.101.x` | URI SAN constraints bypass | Postgres certs use DNS SANs, not URI SANs, in our deployment. Fixed by sqlx 0.8. |
| RUSTSEC-2026-0382 | `rustls-webpki 0.101.x` | Distrusted intermediates accepted | Postgres CA is pinned per-deployment; no untrusted intermediates can be presented. Fixed by sqlx 0.8. |
| RUSTSEC-2024-0363 | `sqlx 0.7.4` | Binary Protocol Misinterpretation | Requires attacker-controlled PostgreSQL server. In prod Postgres is on isolated `backend` network (`internal: true`). Fixed by sqlx 0.8.1. |
| RUSTSEC-2026-0099 | `rustls-webpki 0.101.x` | Wildcard name constraints bypass | Same family as other rustls-webpki: Postgres TLS on isolated network; no wildcard certs trusted. Fixed by sqlx 0.8. |
| RUSTSEC-2026-0381 | `webpki-roots 0.25.4` | Bundled root certs trust distrusted emSign intermediates | `webpki-roots` pulled by reqwest 0.11 (dev-only). Unused at runtime; no outbound HTTPS via reqwest. Fixed by reqwest 0.12. |

**Re-evaluate this list whenever `sqlx` or `reqwest` are bumped to a new
major version.** The tracked upgrade path to remove these ignores is
sqlx 0.7 -> 0.8 and reqwest 0.11 -> 0.12.
