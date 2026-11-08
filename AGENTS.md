# AGENTS.md

These rules are mandatory. Read them before changing anything. When existing code disagrees with them, the rules win. Fix the code you touch.

## Architecture

- **The backend decides and the frontend renders.** Filtering, sorting, pagination, aggregation, validation, eligibility, policy and copy live in Rust (`pipedash-core`, plugins). TypeScript keeps formatting, layout, responsive logic, local UI state (open/closed, selection, unsaved drafts), transport/cache plumbing, platform effects, the log renderer and CSV/PNG export.
- **One adapter path per capability:**
  1. Core service method in its focused file.
  2. Desktop command in `crates/pipedash-desktop/src/commands/<domain>.rs`, registered with a qualified path in `lib.rs`.
  3. Axum handler in `crates/pipedash-web/src/routes/<domain>.rs`.
  4. TS methods in `src/services/{tauri,api}/<family>.ts` and the `PipedashService` interface in `src/services/types.ts`.
  5. Hook in the feature's `queries/`.
- **Transports stay thin.** They only extract inputs, clone `core()` and map errors. They never contain logic, and both transports behave identically.
- **Every action re-checks backend policy.** UI availability is never authorization.
- **Presentation is backend data.** Pipelines and runs come back with `actions`, `display` and `pace`. View pages carry `presentation`, and plugin metadata carries all labels and copy. The core `Presenter` is the only place that calls plugin presentation hooks. Every core egress, including events, goes through it.

## Providers are opaque plugins

- **Outside `crates/pipedash-plugin-<id>/` and `crates/pipedash-plugins/`, nothing names a provider.** That means no provider names, no `provider_type ==` checks, no `"<id>__"` prefixes and no identifiers like `isFlux`, in any Rust (tests included), TS, Swift or Kotlin. `scripts/audit_structure.py` enforces this. Core acts only on metadata, capabilities and hooks.
- **A new provider is one crate plus one line:**
  - `Cargo.toml`: workspace version, edition and authors, `[lints] workspace = true`, and `description` (it is the UI description). Name extra spellings in `[package.metadata.pipedash] aliases = [...]` so the guard catches them.
  - `assets/icon.svg`. The build fails without it.
  - `src/lib.rs`: `pipedash_plugin_api::register_plugin!(MyPlugin);`.
  - `src/metadata.rs`: `PluginMetadata::new("<Display Name>", config_schema, table_schema, capabilities)`, plus only the provider-specific overrides (presentation, run history, token prefixes, permissions, features).
  - Add one dependency line to `crates/pipedash-plugins/Cargo.toml`. Its `build.rs` generates the registry, and CI packaging plus the release matrix discover the crate.
- **Never re-derive manifest data.** Provider id (from the crate name), version, description, authors and icon come from the crate manifest through `register_plugin!`.
- **Plugins use local ids only.** Core composes and strips `<type>__<provider_id>__<local>` and fills `provider_id`/`provider_type`. Never format or parse global ids in a plugin.
- **Override hooks, don't branch.** Provider behavior goes through `Plugin` hooks (`pipeline_actions`, `run_actions`, `run_display`, `trigger_values`, `can_archive_run`, `configured_repositories`, ...). Start from `plugin::defaults` and adjust. Shared protocol code goes in `pipedash-plugin-api` (http, config, pagination, logs, kubernetes), never copied between providers.
- **Contract tests cover every plugin.** `crates/pipedash-plugins/tests/` loops over the registry for metadata, capability consistency, id round-trip and token prefixes. A provider adds unit tests only for its own logic.

## Code rules

- **Size:** non-test `.rs`/`.ts`/`.swift`/`.kt` files have at most 400 lines and `.tsx` at most 300. Indentation is at most 7 levels (4 spaces in Rust/Swift/Kotlin, 2 in TS). Fix depth with guard clauses, `let … else`, `?`, extracted functions and extracted components, never by reformatting.
- **Comments:** none, apart from tool-required ones such as the `# vX.Y.Z` after a SHA pin. Production code carries no lint suppressions (`biome-ignore`, `#[allow]`, `#[expect]`). Fix the code instead.
- **Cutovers:** no shims, aliases, re-exports, barrel `index.ts` files or deprecated paths. Callers import from the defining module. Code, commands, routes and TS methods that lose their last caller are deleted in the same change.
- **Duplication:** shared logic gets one owner next to its layer. `mise run lint:dup` must stay at or below 1%.
- **Rust modules:** use `foo.rs` + `foo/bar.rs`; add no new `mod.rs`. Use the narrowest visibility. Never split one `impl Trait for Type` across files; delegate to `pub(super) <name>_impl`.
- **Rust tests:** live in `<stem>/tests.rs` behind `#[cfg(test)] mod tests;`. No inline `mod tests { }`.
- **Rust safety:** no `unwrap`/`expect` outside tests (clippy denies it). Never hold `std::sync` locks across `.await`. Blocking file, crypto or keyring work goes through `tokio::fs` or `spawn_blocking`. Spawned work is owned and cancellable.
- **Migrations:** never edit files under `crates/pipedash-core/migrations/**`. Schema changes are new forward migrations for both backends.
- **SQL:** multi-line SQL goes in `crates/pipedash-core/sql/**` via `include_str!` with runtime `sqlx::query`. SQLite/PostgreSQL type differences go through `BindValue`, never casts.
- **DTOs:** Rust structs serialize snake_case. New enums use `#[serde(rename_all = "snake_case")]`. TS mirrors are hand-written in `src/types/<domain>.ts`.
- **Frontend:** Biome rules (`noNestedTernary`, `noImportCycles`, no `any`, no non-null assertions) and knip must be clean. Server state lives in TanStack Query, never mirrored into stores.

## Supply chain and CI

- Pin actions by commit SHA with a `# vX` comment. Pin images by digest. Composite actions are referenced as `$/.github/actions/...`.
- Every workflow job has `timeout-minutes`. Never use `continue-on-error`.
- Dependabot owns version updates. `mise run audit:deps` (OSV) runs locally only: the CI scan is paused until Tauri's Linux dependency chain (gtk 0.18, zbus 5) ships fixes. A change must not add new findings. Fix vulnerabilities by upgrading or replacing the dependency, never by ignoring them.

## Before you finish

Run these and make every one pass:

```
mise run fmt:check && mise run lint && mise run check && mise run test:back && bun run test
TEST_DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/pipedash_test mise run test:postgres
```

`mise run lint` includes `lint:structure` (size, depth, inline tests, provider names) and `lint:dup`. A bug fix ships with a regression test that fails without the fix.
