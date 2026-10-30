# CLAUDE.md

Guidance for Claude Code in the ProDeck repo. See README.md for setup and deployment.

## What this is

ProDeck: deck material estimator (pick list + cut list + framing design) for contractors/PMs. Rust Cargo
workspace, API-first: the Axum REST API is the product contract and the Leptos WASM client only uses public endpoints.

## Commands

- `cargo test` - engine unit + scenario tests and API tests (in-memory store). Default members exclude `web`.
- `cargo clippy --all-targets -- -D warnings` and `cargo clippy -p web --target wasm32-unknown-unknown -- -D warnings`
- `cargo fmt --all`
- Run locally: `STORE=memory COOKIE_SECURE=false BOOTSTRAP_ADMIN_EMAIL=... BOOTSTRAP_ADMIN_PASSWORD=... cargo run -p server`
  plus `cd crates/web && trunk serve` (port 8081, proxies `/api`).
- Engine CLI: `cargo run -p estimator --example dump -- <project.json>`
- `scripts/sbom.sh` - regenerate `sbom.txt` after ANY dependency change. `sbom.txt` is local-only: gitignored and
  dockerignored; never commit or publish it.

## Architecture

- `crates/estimator`: pure, deterministic, no I/O. Pipeline in `lib.rs::estimate`: geometry -> framing -> decking ->
  stairs -> railing -> `bom::finish` (cut optimization per `Stock`, packaging per `Item`). Everything is inches.
  Stages push into a shared `Takeoff` (cut pieces, counted items, warnings, tips). Span data lives in
  `data/span_tables.toml` (DCA6; must be verified before permit use). Issued estimates store the `Estimate`
  output as a snapshot, so any field added to it needs a serde default and a fill-in in `Estimate::backfill`
  (see the `estimates_saved_before_new_fields_still_load` scenario), or old snapshots fail to load. `tests/scenarios.rs` asserts hand-checked
  quantities: if an intentional engine change moves them, re-verify by hand before editing the numbers.
- `crates/api-types`: request/response DTOs. Changing them changes the public API and the OpenAPI spec.
- `crates/server`: `routes/` handlers carry `#[utoipa::path]`; add new handlers to both `routes::api()` and the
  `ApiDoc` paths list (the `openapi_lists_every_route` test checks key routes). Persistence goes through the
  `store::Store` trait (Firestore + in-memory); add methods to both impls. Auth: `auth::Auth` extractor accepts
  session cookie or `pdk_` bearer key; credential-management endpoints (keys, passwords, user creation, roles)
  call `require_session()`. Change existing users only via `update_user`/`patch_user` and keys via their field
  methods, never `put_user`/`put_api_key`: whole-record writes race (a sign-in could undo a disable or reset).
- `crates/web`: Leptos 0.8 CSR. `api.rs` is the only network layer. Results views in `pages/views.rs` are shared by
  the editor and print page. CSS is hand-written, mobile-first (`style/app.css`), 44px tap targets.
- `infra/`: Terraform. Cloud Run image is managed by CI (`ignore_changes`), not Terraform.

## Conventions

- No emoji anywhere in code, comments, UI strings, or config.
- Keep the UI usable at 390px wide; check new screens at phone width.
- Every new API capability must be reachable via the REST API, not only the UI.
