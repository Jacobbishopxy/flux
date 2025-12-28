# Milestone 1: Bootstrap

- Goal: bring up the base data stack so Rust services can talk to TimescaleDB.

## Tasks

- [x] TimescaleDB docker-compose with persistent volume and healthcheck (`docker/timescaledb/docker-compose.yml`).
- [ ] Add `sqlx` dependency and a connection pool helper in `flux-db`.
- [ ] Scaffold an initial migrations directory and runner command (e.g., `sqlx migrate`).
- [ ] Document local workflow to start TimescaleDB, apply migrations, and supply connection env vars.
