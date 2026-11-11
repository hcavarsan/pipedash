# Pipedash delivery correlation: definition of done

Pipedash links build pipelines to deployments across providers by observation alone. The user changes nothing in a pipeline, a manifest or a cluster. This document states what must be true for the feature to be done.

## 1. Principles

These seven rules override every other section when they conflict.

1. Pipedash observes. It never asks the user to add a step, a label, an annotation or a trace header.
2. A link exists only when evidence proves it. Missing evidence means unknown, never a guess.
3. Equal values identify an entity. They do not identify the applications that use the entity.
4. A plugin reports what it saw. It never invents an outcome, a time or a duration.
5. The backend decides and the frontend renders. Every label, layout position and evidence sentence comes from Rust.
6. No provider name appears outside the plugin crates. Core acts on observations, capabilities and hooks.
7. A user decision always wins and survives every recompute.

## 2. Vocabulary

The model has seven entities. Each has one canonical identity.

| Entity | Canonical identity |
|---|---|
| Repo | `host/full/path`: lowercase host, default port removed, userinfo removed, `.git` removed, every subgroup kept |
| Commit | Repo identity plus the full 40 or 64 hex SHA |
| Artifact | Image digest `sha256:<hex>`. The repository name is an attribute. A tag is never an identity |
| Target | Cluster identity plus namespace, or the provider's host identity for a deployment that does not run in a cluster |
| Deploy unit | Provider instance plus the provider's own stable ID for the deployed object |
| Pipeline | The existing global pipeline ID |
| Application | An explicit identity, defined in section 5 |

Cluster identity is the UID of the `kube-system` namespace when the plugin has Kubernetes access. Otherwise it is the normalized API server host.

## 3. Plugin contract

Each plugin emits typed, composite observations. Core never reads `metadata` keys or `Pipeline.repository` for correlation.

### 3.1 Observations

`pipedash-plugin-api` owns a closed set of types in `src/correlation.rs`. Each observation binds its own parts, so core never pairs a path with the wrong repo or a namespace with the wrong cluster.

```rust
pub struct Correlation {
    pub unit: Option<UnitId>,
    pub unit_parent: Option<UnitId>,
    pub unit_depends_on: Vec<UnitId>,
    pub app_ref: Option<AppRef>,
    pub app_label: Option<String>,
    pub sources: Vec<SourceObservation>,
    pub targets: Vec<TargetObservation>,
    pub produced: Vec<ArtifactObservation>,
    pub consumed: Vec<ConsumedArtifact>,
}

pub struct SourceObservation {
    pub repo: RepoId,
    pub path_scopes: Vec<PathScope>,
    pub commit: Option<CommitSha>,
}

pub struct TargetObservation {
    pub cluster: ClusterId,
    pub namespace: Option<String>,
    pub name: String,
    pub lifetime: TargetLifetime,
}

pub struct ArtifactObservation {
    pub repository: ArtifactRepository,
    pub digest: Option<Digest>,
    pub tag: Option<String>,
}

pub struct ConsumedArtifact {
    pub artifact: ArtifactObservation,
    pub target: Option<usize>,
}

pub struct AppRef {
    pub authority: String,
    pub scope: String,
    pub id: String,
}
```

`TargetLifetime` is `Standing` or `ShortLived`. `unit_parent` is the provider's own parent object for a set of generated units. `unit_depends_on` lists the units the provider declares this unit depends on; it is the "declared dependency between units" of section 5.6. `app_label` is the value of the provider's application label; it feeds the "same application label value" suggestion of section 5.2 and never creates an application.

- `Pipeline` and `PipelineRun` each gain a `correlation: Correlation` field. Plugins fill it during the existing fetches.
- The subject of an observation is the pipeline or run that carries it. Core adds the provider instance.
- `RepoId`, `CommitSha`, `Digest`, `ClusterId`, `ArtifactRepository` and `PathScope` are newtypes. Only the normalizers in `pipedash-plugin-api` construct them, so a plugin cannot emit a value that is not canonical.
- A pipeline with several sources emits one `SourceObservation` per source. Each keeps its own repo, path scopes and commit.
- A pipeline with several targets emits one `TargetObservation` per target. `ConsumedArtifact.target` is the index of the target that runs the artifact.
- Core compares a path scope or a commit only inside the `SourceObservation` whose repo matched. It compares a namespace only inside the `TargetObservation` whose cluster matched.
- An `ArtifactObservation` with no digest supports suggestions only.
- `repository_name` stays a display helper. Nothing uses it for identity.
- A plugin that cannot produce a canonical value leaves the observation out.
- A plugin sets `app_ref` only for an authoritative, scoped application ID. That is an ID the provider or a software catalog defines as one application across its stages.

### 3.2 Run evidence

Each run states what the provider recorded.

```rust
pub struct RunEvidence {
    pub outcome: Observed,
    pub timing: Observed,
    pub attempt: bool,
}

pub enum Observed { Recorded, Absent }
```

- `outcome: Absent` means the provider keeps no result for this record.
- `timing: Absent` means the provider keeps a single timestamp with no separate start and end.
- `attempt: false` means the record exists only because the deployment succeeded.
- A mapper that builds runs from success-only history sets `Absent`, `Absent` and `false`. It never sets `duration_seconds` or copies current health into past runs.

### 3.3 Resolver hooks

Six hooks let core ask a plugin for evidence on demand. Each defaults to "not supported".

| Hook | Returns |
|---|---|
| `source_hosts(config)` | The repo hosts this provider instance can answer for |
| `resolve_commit(repo, sha_or_prefix)` | Full SHA, authored time, committed time, message, parents |
| `compare_commits(repo, base, head)` | Ahead count, behind count, the commits between |
| `commit_changes(repo, sha)` | Changed paths with their patches |
| `artifact_provenance(artifact)` | Provider-native provenance for a digest: the producing run and its `SourceObservation` |
| `registry_auth(host)` | Credentials this provider instance holds for a registry host |

Core routes a repo question to the provider instance whose `source_hosts` contains the repo's host.

### 3.4 Obligations by plugin class

Every plugin meets the obligations of each class it belongs to.

| Class | Must emit |
|---|---|
| Runs builds from a source repo | One `SourceObservation` per repo on the pipeline, with `path_scopes` when the provider exposes trigger path filters. The same on each run, with `commit` |
| Publishes artifacts or attestations | `produced` on the run, from the provider's package, registry or attestation API |
| Records deployments or environments natively | `targets` and `consumed` on the run that deployed |
| Reconciles a desired state | `unit`, one `SourceObservation` per declared source with its commit, one `TargetObservation` per destination, and `consumed` |
| Has Kubernetes access | `consumed` from the running pods' resolved digests for the workloads the unit owns, each bound to its target |
| Hosts source repos | The four commit hooks and `source_hosts` |

A plugin resolves an indirect source reference to the real repo URL before it builds a `SourceObservation`.

## 4. Automatic evidence collection

Core collects evidence in five ways. None needs a change outside Pipedash.

### 4.1 Provider APIs

Plugins build the observations in section 3 from data their API already returns or from extra read calls. Extra calls pass through the existing quota checks.

### 4.2 Registry read

Core reads an image by digest from its OCI registry. This works from the deployed side, so it needs no knowledge of the build.

- Core reads the image config labels `org.opencontainers.image.source` and `org.opencontainers.image.revision`.
- Core reads attached provenance through the referrers API and extracts the builder run, the repo and the commit.
- Core resolves a tag to a digest only to compare it with a digest it observed running.
- Credentials come from `registry_auth`. A generic registry plugin covers registries no other provider holds credentials for.
- Core never reads pull secrets from a cluster.

### 4.3 Log evidence

Core scans build logs for pushed digests. Pipedash already ingests run logs, so this reuses the log dialects and the ingester.

- The scanner matches push lines from the common image builders. It ignores pull lines and base image digests.
- A scan runs once per concluded run and is cached by `run_hash`.
- A scan is demand-driven. Core scans only when a deployed digest has no producer after 4.1 and 4.2.
- Candidate runs concluded in the 24 hours before Pipedash first observed the digest. Their pipeline has a source whose repo matches the image's source label or the unit's commit lineage. The limit is 50 runs per digest.
- The patterns live in `pipedash-plugin-api` next to the log dialects.

### 4.4 Config commit lineage

Core follows a config repo commit back to the artifact it introduced.

- Core calls `commit_changes` for the revision a unit synced.
- Core extracts each image reference the patch added.
- Core links the config commit to that reference. The link becomes exact when the reference resolves to the digest observed running.

### 4.5 Live observation

Core records every transition it sees on a refresh.

- A transition holds the pipeline, the time, the old and new status, the old and new health, the revision and the running digests.
- Core records an observation window per provider: the continuous spans in which refreshes ran.
- Anything that needs continuity is computed only inside observation windows.

## 5. Correlation rules

Core builds a graph of typed, directed edges. Each edge stores its rule, its evidence and one of two confidence levels: exact or suggested.

### 5.1 Exact edges

| Edge | Rule |
|---|---|
| Run `built_from` commit | The repo and commit of one `SourceObservation` on the run |
| Run `produced` artifact | A `produced` digest, an attestation naming the run, or a pushed digest in the run's log |
| Artifact `built_from` commit | An attestation, or a revision label on a digest that a `produced` edge also covers |
| Unit `runs` artifact | A `consumed` digest |
| Unit `synced` commit | The repo and commit of one `SourceObservation` on the unit |
| Config commit `introduced` artifact | Section 4.4, when the digests match |
| Unit `deploys_to` target | Each `TargetObservation` on the unit |

### 5.2 Suggested edges

| Edge | Rule |
|---|---|
| Artifact `built_from` commit | A source or revision label with no `produced` edge behind it |
| Artifact `built_from` commit | A tag that contains a SHA prefix of 7 or more hex characters that `resolve_commit` resolves to exactly one commit |
| Unit `same_application` unit | Same artifact repository and the same digest seen running in both units |
| Unit `same_application` unit | One source in each unit with the same repo and overlapping path scopes |
| Unit `same_application` unit | Same application label value |
| Unit `same_application` unit | Same name after environment tokens are removed |

Timing alone never creates an edge.

### 5.3 Hubs

An entity linked to more than five units is a hub. A hub supports no suggestion. Config repos, base images, clusters and shared namespaces fall under this rule without being named.

### 5.4 Applications

An application is created and merged only by an identity source. Shared entities never merge two applications.

1. Each deploy unit gets its own application. Its identity is the unit ID. Its name is the unit's name.
2. Units that emit the same `AppRef` share one application.
3. A user override creates, merges, splits, renames or hides an application.
4. An `[applications.<key>]` table in `config.toml` defines an application.

A higher number overrides a lower one.

A repo and path pair is not an application identity. An application label is not an application identity. Both are suggestion evidence only.

- A build pipeline with no unit gets no automatic application. One application per pipeline would repeat the pipeline list.
- Config or a user override creates a build-only application.
- When a unit disappears from its provider, an application with no other unit is archived.
- An archived application leaves the list. Its history stays until metrics retention removes it.

### 5.5 Membership

A pipeline belongs to an application through an exact chain. Membership is many-to-many.

- A build pipeline joins when one of its runs produced an artifact the application's unit runs.
- A build pipeline joins when one of its sources and one of the unit's sources have the same repo and overlapping path scopes.
- A build pipeline whose matching source has no path scope, in a repo that has more than one build pipeline, is a suggestion.
- A run that both builds and deploys fills both stages.
- A pipeline with no exact chain to any unit stays outside every application until the user maps it.

Stage is derived. A pipeline that emits `unit` or `targets` is a deploy stage. Any other member is a build stage.

The user or config can set a member's stage. That choice is an override.

### 5.6 Environments

An environment is the display name for a target or a group of targets inside one application. No environment list is global, and no environment name is built in.

- The name comes from the provider's own name for the cluster and namespace.
- Each application owns its targets. Two applications that deploy to one namespace each keep their own name and grouping for it.
- Targets have no order by default. The Flow tab shows them side by side.
- A direction between two targets exists only on provider evidence, a user override or config. Provider evidence is a declared dependency between units or a promotion the provider itself records.
- Chronology never creates a direction. Pipedash commits one provider at a time, so the order in which it first saw a digest reflects polling.
- A provider-recorded deployment time can show as a labeled hint: "deployed 12 minutes after `qa`". The hint draws no arrow and feeds no metric.
- Targets group automatically when their units share a `unit_parent`. The group shows as one step with a count.
- A target is `ShortLived` when the provider ties it to a pull request or a temporary branch. Short-lived targets collapse into one entry. The metrics in section 7 leave them out.
- An application with more than six targets or groups shows one summary by health in the list.
- The user and config can rename, group and ungroup targets, and set or remove a direction.

### 5.7 Resolution

Resolution is incremental and deterministic.

- It runs in `apply_pipelines`, after `update_pipelines_cache` commits and before events publish.
- It touches only subjects whose observations changed in that provider's refresh.
- The same observations and overrides always produce the same graph.
- Enrichment from sections 4.2 to 4.4 runs in owned, cancelable background tasks. It never delays the publication of pipeline status.
- When the graph changes, core emits `applications-changed`.

### 5.8 Overrides

Overrides are stored apart from derived data, each with its author and time.

| Kind | Effect |
|---|---|
| Link | Attaches a pipeline to an application, with a stage |
| Unlink | Removes an automatic link or rejects a suggestion. It never comes back |
| Create | Makes an application from chosen pipelines |
| Merge | Combines two applications |
| Split | Moves units out into a new application |
| Rename | Sets the application's name |
| Hide | Removes an application from the list. Its data stays |
| Stage | Sets a member's stage |
| Target name, target group, direction | Sets how one application shows its targets |

- The application page lists every override that applies to it. Each has a revert action.
- Reverting deletes the override. The next resolution restores the derived state.
- A user-created application can be deleted. An automatic one can only be hidden, because resolution would create it again.
- The backend computes `ApplicationCapabilities { edit }` for each application, as views do with `ViewCapabilities`. Every mutation re-checks it.
- An application defined in `config.toml` is read-only in the UI, as config views are.

### 5.9 Configuration as code

`config.toml` defines applications next to `[providers]` and `[views]`. A user who keeps the file never repeats a decision in the UI.

```toml
[correlation]
log_evidence = true
registry_read = true

[[correlation.ignore]]
provider = "gitops"
pipeline = "sandbox-*"

[applications.checkout]
name = "Checkout API"

[[applications.checkout.members]]
provider = "ci"
pipeline = "acme/checkout/build"
stage = "build"

[[applications.checkout.members]]
provider = "gitops"
pipeline = "checkout-*"

[[applications.checkout.exclude]]
provider = "gitops"
pipeline = "checkout-legacy"

[applications.checkout.targets.qa]
name = "QA"
namespaces = ["checkout-qa"]

[applications.checkout.targets.production]
name = "Production"
clusters = ["prod-*"]
namespaces = ["checkout"]
after = ["qa"]

[applications.shared-lib]
name = "Shared library"
hidden = false

[[applications.shared-lib.members]]
provider = "ci"
pipeline = "acme/shared-lib/*"
```

- `provider` is a key under `[providers]`. `pipeline` is a case-sensitive glob on the provider-local pipeline ID.
- `stage` is `build` or `deploy`. Without it, stage is derived.
- Exact chains still add members to a config application. `exclude` removes one.
- A unit that a config application claims gets no automatic application.
- A target table matches that application's targets by `clusters` and `namespaces` globs. A table that matches several targets forms a group.
- `after` sets a direction from the named target tables to this one.
- A table's `clusters` x `namespaces` is a product, so a group of targets that is not one needs several tables. A table with `group = "<table key>"` adds its targets to the group of that table, which owns the group's `name` and `after`; the member table sets no `after`, may omit `name`, and an `after` that names either table means the same group. `group` must name another table of the same application that has no `group` itself. A target with no namespace matches `namespaces = [""]`. Copy as TOML writes a product group as one table and any other group as an owner table plus member tables, with each direction once.
- `[[correlation.ignore]]` stops matching pipelines from creating applications or suggestions.
- `[correlation]` turns log evidence and registry read on or off. Both default to `true`.
- The existing `ConfigWatcher` detects a change within its 5 second poll.
- A reload is atomic. Core parses the file, validates every application table and resolves selector ownership against the current pipelines before it commits anything.
- Validation rejects an unknown provider key, an unknown stage, a cycle in `after` and one unit claimed by two applications.
- A rejected file changes nothing. The last valid config stays active, with every membership, target, direction and override as it was.
- A file that fails to parse is rejected the same way, such as one an editor saved halfway.
- A commit applies the whole new config in one database transaction, and core emits `applications-changed` once.
- The watcher today only logs a failed apply. This feature adds the stored last valid config and a visible error state.
- A rejected file shows a banner on the Applications page with the backend's error text and the time. The banner clears on the next valid load.
- Core checks ownership again when a new pipeline matches a glob. If the pipeline now matches two applications, neither claims it.
- That pipeline keeps its automatic application, both config applications show the conflict, and the rest of the graph stays unchanged.
- A selector that matches nothing is a warning on the application page, because a pipeline can appear later.
- These tables hold no secret.
- Each application made in the UI offers "Copy as TOML". The backend renders the text.
- Loading that text as config produces the same members, targets and directions.
- When the file and a UI override disagree, the file wins. The override list marks that override as shadowed.

## 6. Storage

All schema changes are new forward migrations for SQLite and PostgreSQL. Multi-line SQL lives in `crates/pipedash-core/sql/correlation/`.

| Table | Key columns | Purpose |
|---|---|---|
| `source_observations` | `subject_kind`, `subject_id`, `run_number`, `ordinal` | One row per source: `repo`, `path_scopes_json`, `commit_sha`. Indexed on `repo` and on `(repo, commit_sha)` |
| `target_observations` | `subject_kind`, `subject_id`, `run_number`, `ordinal` | One row per target: `cluster`, `namespace`, `name`. Indexed on `(cluster, namespace)` |
| `artifact_observations` | `subject_kind`, `subject_id`, `run_number`, `relation`, `ordinal` | One row per artifact: `repository`, `digest`, `tag`, `target_ordinal`. Indexed on `digest` and on `repository` |
| `subject_identities` | `subject_kind`, `subject_id` | `unit_id`, `app_ref_authority`, `app_ref_scope`, `app_ref_id` |
| `correlation_edges` | `from_kind`, `from_id`, `to_kind`, `to_id`, `relation` | Stores `confidence`, `rule`, `evidence_json`, `first_seen`, `last_seen` |
| `applications` | `id`, `identity_kind`, `identity_key` unique | Stores `name`, `hidden`, `archived_at`. `identity_kind` is `unit`, `app_ref`, `user` or `config` |
| `application_members` | `application_id`, `pipeline_id`, `stage` | Stores `source` and `edge_id` |
| `application_overrides` | `id`, `application_id`, `kind`, `subject`, `target` | Stores `author` and `created_at` |
| `application_targets` | `application_id`, `target_key` | Stores `name`, `group_key` and `lifetime` |
| `target_directions` | `application_id`, `from_group`, `to_group` | Stores `source`: `provider`, `user` or `config` |
| `artifacts` | `digest` | Stores `repository`, `first_seen`, label source and label revision |
| `changes` | `application_id`, `repo`, `commit_sha` | Stores `authored_at` |
| `change_stages` | `change_id`, `stage`, `environment_id`, `pipeline_id`, `run_number` | Stores `started_at`, `concluded_at`, `status`, `outcome_recorded`, `timing_recorded`, `attempt` |
| `observed_transitions` | `pipeline_id`, `at` | Stores status, health, revision and digests before and after |
| `observation_windows` | `provider_id`, `started_at` | Stores `ended_at` |
| `run_evidence` | `pipeline_id`, `run_number` | Stores `outcome_recorded`, `timing_recorded`, `attempt`, `started_at`, `concluded_at` and `status` for each observed run |

When a pipeline joins an application, core fetches its run history in the background. A member never waits for the user to open it.

## 7. Metrics

The unit is a change: one commit traced through each stage, with what was recorded at each. Core computes a metric only when every input it needs was recorded.

| Metric | Definition | Requires |
|---|---|---|
| Lead time | Commit authored time to the first healthy deployment in an environment | `resolve_commit` and a deployed time |
| Delivery time | First build run start to the first healthy deployment in an environment | Recorded build timing and a deployed time |
| Build time | Start to end of the build stage | Recorded timing |
| Wait time | Start of the downstream stage minus end of the upstream stage, on one proven attempt chain. Between two targets only when a direction exists | Recorded end upstream and recorded start downstream |
| Build to deployed | End of the build stage to a target's deployed time. It includes the deployment itself and never stacks with wait time or deploy time | Recorded build timing and a deployed time |
| Deploy time | Start to end of the deploy stage | Recorded timing on an attempt record |
| Deploy frequency | Deployments per period per environment | A deployed time |
| Build success rate | Successful build runs over concluded build runs | Recorded outcome |
| Deploy failures | Failed deploy attempts over all deploy attempts | Attempt records with a recorded outcome |
| Change failure rate | Deployed changes that failed in the environment over all deployed changes | An observation window that covers the change |
| Recovery time | Failure event to the next healthy state | An observation window that covers both events |
| Funnel | Changes built, passed, and deployed to each environment | Exact chains for each stage counted |
| Drift | Commits between an environment's live commit and the newest green build | `compare_commits` |

A deployed change failed in the environment when one of these was observed:

- A deploy attempt for it ended failed.
- The unit's health went to degraded or failed within 60 minutes of the deployment while that change was live.
- The next deployment went back to an ancestor commit or to a digest deployed earlier.

Rules for every metric:

- The result is a value or `unavailable` with a reason. It is never zero by default.
- Every result carries coverage: the changes counted, the changes left out, and the date collection began.
- Aggregations and periods reuse the existing ones: `Avg`, `Sum`, `Min`, `Max`, `P95`, `P99` over hourly, daily, weekly and monthly periods.
- A success-only record counts as "deployed at this time" and nothing else.
- Lead time and delivery time report the 50th and 95th percentiles.

## 8. User experience

The user sees one new object: the application. The graph is never shown.

### 8.1 Applications index at `/applications`

The index is a list. Each row is an application with its own targets as chips.

- A chip shows the target or group name, the live commit, the health and how many commits it is behind.
- Two chips have an arrow between them only when a direction exists.
- A group chip shows its count. A row past the six-entry limit shows one health summary chip.
- A build-only application shows its last build result.
- The list updates on `applications-changed` without a reload.
- Sorting, search and the filters for health, behind, provider and open suggestions run in the backend.
- A counter shows open suggestions and opens the review list.

### 8.2 Application detail at `/applications/:applicationId`

The detail page has three tabs in the URL as `?tab=`.

- Flow shows a lane: source, build, artifact, then that application's targets. Targets with a direction run left to right. Targets without one stack in one column. The backend sends each node's column and row. The frontend draws SVG.
- Changes shows one row per commit and one column per stage, with a status dot per cell. A dot opens the existing run modal.
- Metrics shows the metrics in section 7 through the existing chart chain and the existing CSV and PNG export.

### 8.3 Links

- A solid line is an exact edge. A dashed line is a suggestion. There is no third state.
- Every line answers "why linked?" in one sentence the backend wrote, with an unlink action.
- A suggestion offers accept and reject. The review list offers "accept all" for suggestions that have two or more independent rules behind them.
- A suggested sibling unit appears dashed in the Flow tab before the user decides.

### 8.4 Existing pages

- Pipeline detail gains a chip per application the pipeline belongs to.
- The run modal gains "Where is this commit": each environment and whether the commit is live, behind or not deployed.
- A failed deployment gains "Changes in this deploy": the commits between the last live commit and this one.
- A pipeline outside every application looks and behaves as it does today.

### 8.5 Unknown states

- An unavailable metric shows its reason in place of a number.
- A stage with no evidence shows as an empty outline, not as a failure.
- Coverage appears next to every metric.

### 8.6 Layouts

- Desktop registers the page in `NavbarTree` and `NavbarRail`.
- The mobile shell reaches it from Browse. There is no fifth tab.
- On narrow screens the lane stacks top to bottom and each list row becomes a card.
- `resolvePageTitle`, `ownChrome` and the native deep link parser know both routes.

### 8.7 Adjusting

- An Edit action on the application page offers rename, hide, merge, split, member changes, stage, and target name, group and direction.
- Every adjustment is a menu action. None needs a drag, so each works on a phone.
- An Overrides panel lists each override with its author, its time and a revert action.
- "Copy as TOML" puts the application's `config.toml` tables on the clipboard.
- A config-defined application shows "Defined in config.toml" and no edit action.

## 9. Architecture

The feature follows the single adapter path with no logic in a transport.

| Layer | Location |
|---|---|
| Vocabulary, normalizers, hooks, log patterns | `crates/pipedash-plugin-api/src/correlation.rs` and `correlation/` |
| Resolver, applications, changes, metrics | `crates/pipedash-core/src/application/services/correlation_service.rs` and `correlation_service/` |
| Domain types | `crates/pipedash-core/src/domain/application.rs` |
| Repositories | `crates/pipedash-core/src/infrastructure/database/correlation_repository.rs` |
| Desktop commands | `crates/pipedash-desktop/src/commands/applications.rs` |
| Web routes | `crates/pipedash-web/src/routes/applications.rs`, mounted at `/applications` |
| TS services | `src/services/tauri/applications.ts`, `src/services/api/applications.ts`, an `ApplicationsService` in `src/services/types.ts` |
| TS types | `src/types/applications.ts` |
| Feature | `src/features/applications/` with `components/`, `queries/`, `hooks/`, `lib/` and `routes/` |

- Every application DTO passes through the `Presenter`, so member pipelines carry `actions`, `display` and `pace`.
- Both transports return identical data for identical input.
- Every mutation re-checks policy in the backend.
- Server state lives in TanStack Query.
- The lane and the list add no frontend dependency.
- Every file meets the size and depth limits in `AGENTS.md`.

## 10. Security

- Observations hold no token. The repo normalizer removes userinfo before anything is stored.
- Registry credentials stay in the vault with the provider credentials.
- Core sends a registry credential only to the host `registry_auth` named and to the token hosts the credential owner lists in `token_hosts`. A token realm on any other host gets an anonymous token request. A bearer secret goes only to the registry host.
- Evidence text shown to the user holds no credential and no raw log line beyond the matched digest.

## 11. Tests

The feature is done when these tests exist and pass on both databases.

### 11.1 Contract tests in `crates/pipedash-plugins/tests/`

- Every observation a plugin emits holds only canonical newtypes.
- A `ConsumedArtifact.target` index points at an existing entry in `targets`.
- A plugin with two sources and two targets emits two bound observations of each, in a stable order.
- A run with `timing: Absent` has no `duration_seconds`.
- A plugin that declares `source_hosts` implements the four commit hooks.

### 11.2 Resolver tests

- Two services in one repo stay two applications.
- Two units that share a config repo stay two applications.
- Two units that share a base image stay two applications.
- Two units with the same label and no `AppRef` produce a suggestion and no merge.
- Two units with the same `AppRef` share one application.
- A unit with no source gets its own application and no build member.
- A unit with sources `(repo A, path /x)` and `(repo B, path /y)` does not match a pipeline on `(repo A, path /y)`.
- A unit with sources `(repo A, commit 1)` and `(repo B, commit 2)` gets no `synced` edge to `(repo A, commit 2)`.
- A unit with targets `(cluster 1, namespace a)` and `(cluster 2, namespace b)` gets no edge to `(cluster 1, namespace b)`.
- A digest consumed in one target of a unit is not reported as live in the unit's other target.
- A hub supports no suggestion.
- A rejected suggestion does not return after a recompute.
- A merge override survives a recompute and a provider removal.
- A tag that resolves to two commits produces no edge.
- A base image digest in a log produces no `produced` edge.
- A build pipeline with no unit creates no application.
- Two applications that deploy to one namespace keep separate target names.
- Two targets with different first-seen times get no direction.
- Units with one `unit_parent` form one group.
- A short-lived target is left out of deploy frequency.
- Reverting an unlink restores the automatic link.
- A hidden application stays hidden after a recompute.
- An application whose only unit disappeared is archived and keeps its changes.

### 11.3 Metric tests

- A success-only history gives `unavailable` for deploy time, deploy failures and recovery time.
- Change failure rate counts only changes inside an observation window.
- A rollback to an earlier digest marks the rolled-back change as failed.
- Coverage counts match the changes included and left out.
- With build end 10:00, deploy start 10:05 and deploy end 10:15, wait time is 5 minutes and deploy time is 10 minutes.
- With build end 10:00 and only a deployed time of 10:15, wait time is `unavailable` and build to deployed is 15 minutes.
- Build time, wait time and deploy time for one change sum to the span from build start to deploy end.

### 11.4 Frontend tests

- The Flow tab renders the positions the backend sent.
- An unavailable metric renders its reason and no number.
- A row with a direction renders an arrow. A row without one renders none.
- A config-defined application renders no edit action.

### 11.5 Configuration tests

- An `[applications.<key>]` table creates a read-only application, and its claimed unit has no automatic application.
- A glob member picks up a pipeline that appears after the file loaded.
- `exclude` removes a member that an exact chain added.
- A cycle in `after` rejects the file.
- One unit claimed by two applications rejects the file.
- A valid file change reaches the application list within one watcher poll, with one `applications-changed` event.
- A file that fails to parse leaves every membership, target, direction and override unchanged, and the error banner appears.
- A file with one invalid application table applies none of its other tables.
- A valid file after a rejected one applies in full and clears the banner.
- A new pipeline that matches the globs of two applications is claimed by neither, and both show the conflict.
- A failure in the middle of a commit rolls back to the previous config.
- The "Copy as TOML" text, loaded as config, gives the same members, targets and directions.
- A UI override that the file contradicts is marked shadowed, and the file's value applies.
- `[[correlation.ignore]]` stops a matching unit from creating an application.

## 12. Out of scope

- Trace context propagation through build pipelines. It needs a change in the user's pipeline, which principle 1 forbids.
- A free-form diagram the user can rearrange.
