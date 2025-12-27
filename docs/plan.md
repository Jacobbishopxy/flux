# Data Service Plan

## Objectives

- Persist second-level financial market data in TimescaleDB; read/write via `sqlx`.
- Expose an async Rust data service (`tokio`) that ingests from and broadcasts to ZeroMQ.
- Allow chunked historical fetches and live/backtest streams; serialize payloads with FlatBuffers when needed.

## Architecture Overview

- Rust workspace target: core crate for domain types, DB access layer using `sqlx`, and service layer built on `tokio`.
- TimescaleDB (Postgres) as the single source of truth; migrations managed in-repo.
- ZeroMQ sockets for interop: SUB for external feeds, PUB for live/backtest outflow, and a request/response path for chunked fetches.
- FlatBuffers schemas define the on-wire envelope plus message bodies (tick/trade/bar); generated Rust bindings are reused on both ingest and egress.

## Database Design (TimescaleDB)

- Hypertables (timestamptz `ts` primary for ordering):
  - `ticks(ts, symbol, bid, ask, bid_size, ask_size, last, last_size, source, seq_id, ingest_ts)`.
  - `trades(ts, symbol, price, size, side, source, seq_id, ingest_ts)`.
  - Optional `bars_1s(ts, symbol, open, high, low, close, volume, vwap, source)` as rollups.
- Constraints/indexes: unique (symbol, ts, source, seq_id) to dedup; indexes on (symbol, ts DESC); partial indexes on recent windows if needed.
- Policies: compress ticks/trades after N hours; drop raw after retention (e.g., 90d) while keeping rolled-up bars; continuous aggregates for 1s/1m bars.
- Access patterns: parameterized queries for time ranges; server-side cursor/`fetch` to stream chunked results via `sqlx`.

## Messaging (ZeroMQ + FlatBuffers)

- Sockets:
  - `ingest_sub`: SUB to upstream feeds (live/backtest input).
  - `live_pub`: PUB for immediate rebroadcast of validated ticks/trades.
  - `backtest_pub`: PUB for historical replay.
  - `chunk_rep`: REP (or ROUTER/DEALER) for chunked fetch requests.
- Topics & envelopes: topic convention `market.<feed>.<symbol>.<type>`; envelope fields include `msg_type`, `symbol`, `source`, `seq_id`, `ts`, and payload bytes (FlatBuffers).
- Serialization: FlatBuffers schemas for `Tick`, `Trade`, `Bar`, plus `StreamEnvelope` to carry type/version; keep backward-compatible versioning in the envelope.

## Data Flows

- Ingest: `ingest_sub` → decode FlatBuffer → validate & dedup (seq_id/window cache) → buffer per symbol → batched `sqlx` inserts (transactional) → tee to `live_pub`.
- Live broadcast: Tee from ingest; optional LISTEN/NOTIFY or polling to push DB-only arrivals; ensure backpressure via bounded channels.
- Backtest replay: Query DB by time/symbol with `sqlx` streaming cursor → serialize → publish on `backtest_pub` with pacing controls.
- Chunked fetch: `chunk_rep` handles requests `{symbol, start, end, chunk_size}` → stream rows in chunks → FlatBuffers chunk message; support resume tokens if needed.

## Service Surfaces (Rust)

- `db`: connection pool setup, migrations runner, typed queries (insert_tick, insert_trade, fetch_ticks_range, fetch_bars_range) using `sqlx`.
- `ingest`: ZMQ consumer tasks (wrap blocking ZMQ in `tokio::task::spawn_blocking` or use async binding), validation, batching.
- `broadcast`: PUB sockets for live/backtest; publisher tasks consuming channels fed by ingest and backtest query streams.
- `api/chunk`: request handler over ZMQ (REQ/REP or ROUTER) that streams chunked DB reads.
- `schemas`: FlatBuffers `.fbs` files and generated Rust modules.

## Observability & Ops

- Metrics: ingress/egress rates, DB latency, batch sizes, dedup hits, replay lag.
- Logging: structured logs with request IDs and symbols; debug sampling for payloads.
- Reliability: ZMQ reconnect/backoff, sequence gap detection, WAL-friendly batch sizes, graceful shutdown draining channels.
- Local stack: docker-compose for TimescaleDB + optional sample feed generator; `sqlx` offline feature for compile-time query checks; migrations CLI.
- Testing: unit tests for decoding/validation; integration tests against ephemeral Postgres/Timescale for inserts and chunked reads; benchmarks for ingest and query throughput.

## Milestones

1) Bootstrap: docker-compose for TimescaleDB, `sqlx` pool setup, migrations scaffold.
2) Schema: hypertables, constraints, policies, and initial migrations; `sqlx` query coverage.
3) FlatBuffers: define `.fbs`, generate Rust, integrate envelope helpers.
4) Ingest path: ZMQ SUB integration, validation/dedup, batched writes, live PUB tee.
5) Backtest/chunked read: streaming queries, pacing, REP/RTR handler, backtest PUB.
6) Hardening: metrics/logging, reconnection/replay logic, retention/compression policies, perf tuning.
