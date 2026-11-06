# Contributing

Thanks for taking the time to contribute!

## Before you start

- Open an issue describing the change (bug fix, feature, docs). This avoids
  duplicate work and lets maintainers weigh in on the approach early.
- For security issues, see [SECURITY.md](SECURITY.md) — do **not** file a
  public issue.

## Development setup

### Prerequisites

- Rust toolchain (stable): <https://rustup.rs>
- PostgreSQL 16 (local, Docker, or `docker run postgres:16-alpine`)
- Docker + Docker Compose (optional, for the full stack)

### Running locally

    cp .env.example .env
    # edit .env: set DATABASE_URL, ADMIN_API_KEY (16+ chars)
    cargo run

### Running tests

Integration tests require a reachable Postgres. Set `TEST_DATABASE_URL`
(or `DATABASE_URL`) and run:

    export TEST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/ecommerce_test"
    cargo test -- --test-threads=1

`--test-threads=1` matters: each test TRUNCATEs shared tables, so parallel
execution would cause flaky failures.

## Coding standards

- **Formatting**: `cargo fmt --all` before committing.
- **Linting**: `cargo clippy --all-targets --all-features -- -D warnings` must pass.
- **Tests**: every bug fix should come with a regression test; every new
  endpoint should have at least one integration test.
- **Commit messages**: follow [Conventional Commits](https://www.conventionalcommits.org)
  (`feat:`, `fix:`, `test:`, `chore:`, `docs:`, `ci:`). Use the commit body
  to explain *why*, not just *what*.

## Pull request checklist

- [ ] `cargo fmt --all` — no diff
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` — clean
- [ ] `cargo test -- --test-threads=1` — all green
- [ ] New behaviour covered by tests
- [ ] No new `unwrap()` / `expect()` on user input or I/O
- [ ] Docs / README updated if the public API changed
- [ ] No secrets committed (`.env`, keys, tokens)

## Code review

- Maintainer reviews within ~3 business days.
- Address comments, push fixups, then `git rebase -i` to tidy before merge
  (or squash-merge — either is fine).

## License

By contributing, you agree that your contributions will be licensed under the
MIT License (see [LICENSE](LICENSE)).
