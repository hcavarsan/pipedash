import type { AggregationPeriod } from './metrics'
import type { Pipeline, PipelineStatus } from './pipelines'
import type { SortDirection } from './views'

export type IdentityKind = 'unit' | 'app_ref' | 'user' | 'config'
export type Stage = 'build' | 'deploy'
export type MemberSource = 'exact' | 'override' | 'config'
export type Confidence = 'exact' | 'suggested'

export interface ApplicationCapabilities {
  edit: boolean
  delete: boolean
}
export type ChipKind = 'target' | 'group' | 'short_lived' | 'summary'

export interface TargetChip {
  group_key: string
  label: string
  commit_short: string | null
  status: PipelineStatus
  health_label: string | null
  behind_label: string | null
  count: number
  kind: ChipKind
}

export interface ChipArrow {
  from: string
  to: string
}

export interface BuildSummary {
  label: string
  status: PipelineStatus
}

export interface ApplicationSummary {
  id: number
  name: string
  identity_kind: IdentityKind
  capabilities: ApplicationCapabilities
  chips: TargetChip[]
  arrows: ChipArrow[]
  build: BuildSummary | null
  suggestion_count: number
  hidden: boolean
  warnings: string[]
}

export interface ConfigLoadError {
  message: string
  at: string
}

export interface LabelOption {
  value: string
  label: string
}

export interface ApplicationCopy {
  title: string
  search_placeholder: string
  empty_title: string
  empty_description: string
  new_application: string
  suggestions: string
  suggestions_empty: string
  health_filter: string
  provider_filter: string
  behind_only: string
  with_suggestions: string
  include_hidden: string
  sort: string
  sort_options: LabelOption[]
  health_options: LabelOption[]
  config_error_title: string
  config_notice: string
  tab_flow: string
  tab_changes: string
  tab_metrics: string
  flow_empty: string
  changes_empty: string
  metrics_empty: string
  legend_exact: string
  legend_suggested: string
  why_linked: string
  edit: string
  rename: string
  hide: string
  unhide: string
  merge: string
  split: string
  members: string
  targets: string
  delete: string
  copy_toml: string
  copied: string
  overrides: string
  overrides_empty: string
  revert: string
  shadowed: string
  accept: string
  reject: string
  accept_all: string
  unlink: string
  save: string
  cancel: string
  name: string
  stage: string
  stage_build: string
  stage_deploy: string
  add_member: string
  remove: string
  group: string
  ungroup: string
  direction: string
  direction_from: string
  direction_to: string
  add_direction: string
  all_targets: string
  funnel: string
  export: string
  load_more: string
  commit_column: string
  warnings: string
  back: string
  hidden: string
  sort_ascending: string
  sort_descending: string
}

export interface ApplicationList {
  applications: ApplicationSummary[]
  total: number
  suggestion_count: number
  config_error: ConfigLoadError | null
  copy: ApplicationCopy
}

export type ApplicationSort = 'name' | 'health' | 'behind' | 'last_deploy'

export interface ApplicationListQuery {
  search: string | null
  health: PipelineStatus[]
  behind_only: boolean
  provider_ids: number[]
  with_suggestions: boolean
  include_hidden: boolean
  sort: ApplicationSort
  direction: SortDirection
}

export interface ApplicationRef {
  id: number
  name: string
}

export interface ApplicationTarget {
  target_key: string
  name: string
  group_key: string
  group_label: string
  short_lived: boolean
}

export type FlowNodeKind = 'source' | 'build' | 'artifact' | 'target' | 'group' | 'short_lived'

export interface FlowNode {
  id: string
  kind: FlowNodeKind
  column: number
  row: number
  title: string
  subtitle: string | null
  status: PipelineStatus | null
  pipeline_id: string | null
  count: number
}

export interface FlowLink {
  id: string
  from: string
  to: string
  confidence: Confidence
  why: string
  hint: string | null
  edge_id: number | null
  suggestion_id: string | null
  unlink_pipeline_id: string | null
}

export interface Flow {
  columns: number
  rows: number
  nodes: FlowNode[]
  links: FlowLink[]
}

export interface ApplicationMember {
  pipeline: Pipeline
  stage: Stage
  source: MemberSource
  why: string
}

export type SuggestionKind = 'same_application' | 'membership' | 'built_from'

export interface Suggestion {
  id: string
  application_id: number | null
  kind: SuggestionKind
  title: string
  why: string[]
  bulk_accept: boolean
}

export type OverrideKind =
  | 'link'
  | 'unlink'
  | 'create'
  | 'merge'
  | 'split'
  | 'rename'
  | 'hide'
  | 'stage'
  | 'target_name'
  | 'target_group'
  | 'direction'

export interface OverrideEntry {
  id: number
  kind: OverrideKind
  description: string
  author: string | null
  created_at: string
  shadowed: boolean
}

export interface ApplicationDetail {
  summary: ApplicationSummary
  flow: Flow
  members: ApplicationMember[]
  targets: ApplicationTarget[]
  suggestions: Suggestion[]
  overrides: OverrideEntry[]
  toml: string | null
  config_defined: boolean
  copy: ApplicationCopy
}

export interface ChangeColumn {
  key: string
  label: string
}

export type CellState = 'success' | 'failed' | 'running' | 'no_evidence' | 'live'

export interface ChangeCell {
  column: string
  state: CellState
  pipeline_id: string | null
  run_number: number | null
  label: string | null
}

export type SegmentKind = 'build' | 'wait' | 'deploy' | 'build_to_deployed'

export interface TimeSegment {
  kind: SegmentKind
  seconds: number
  label: string
}

export interface ChangeRow {
  commit_sha: string
  commit_short: string
  message: string | null
  cells: ChangeCell[]
  segments: TimeSegment[]
}

export interface ChangesPage {
  columns: ChangeColumn[]
  rows: ChangeRow[]
  has_more: boolean
  next_cursor: string | null
}

export type LocationState = 'live' | 'behind' | 'not_deployed'

export interface CommitLocation {
  application_id: number
  application_name: string
  target_label: string
  state: LocationState
  label: string
}

export interface CommitLocations {
  title: string
  entries: CommitLocation[]
}

export interface DeployCommit {
  sha: string
  short: string
  message: string | null
  authored_at: string | null
}

export interface DeployChanges {
  title: string
  commits: DeployCommit[]
  unavailable_reason: string | null
}

export type ApplicationEdit =
  | { type: 'link'; pipeline_id: string; stage?: Stage | null }
  | { type: 'unlink'; pipeline_id: string }
  | { type: 'reject_suggestion'; suggestion_id: string }
  | { type: 'accept_suggestion'; suggestion_id: string }
  | { type: 'merge'; other_application_id: number }
  | { type: 'split'; pipeline_ids: string[]; name: string }
  | { type: 'rename'; name: string }
  | { type: 'hide'; hidden: boolean }
  | { type: 'stage'; pipeline_id: string; stage: Stage }
  | { type: 'target_name'; group_key: string; name: string }
  | { type: 'target_group'; target_keys: string[]; group_name?: string | null }
  | { type: 'direction'; from_group: string; to_group: string; present: boolean }

export interface CreateApplicationMember {
  pipeline_id: string
  stage?: Stage | null
}

export interface CreateApplicationInput {
  name: string
  members: CreateApplicationMember[]
}

export interface ApplicationMetricsQuery {
  application_id: number
  group_key?: string | null
  from: string
  to: string
  period: AggregationPeriod
}

export type MetricId =
  | 'lead_time'
  | 'delivery_time'
  | 'build_time'
  | 'wait_time'
  | 'build_to_deployed'
  | 'deploy_time'
  | 'deploy_frequency'
  | 'build_success_rate'
  | 'deploy_failures'
  | 'change_failure_rate'
  | 'recovery_time'
  | 'drift'

export type MetricUnit = 'seconds' | 'percent' | 'count' | 'commits'

export interface Coverage {
  counted: number
  excluded: number
  since: string | null
  label: string
}

export interface MetricPoint {
  timestamp: string
  value: number
  count: number
}

export interface MetricResult {
  id: MetricId
  label: string
  unit: MetricUnit
  value: number | null
  p50: number | null
  p95: number | null
  unavailable_reason: string | null
  coverage: Coverage
  series: MetricPoint[]
}

export interface FunnelStep {
  key: string
  label: string
  count: number
}

export interface ApplicationMetrics {
  metrics: MetricResult[]
  funnel: FunnelStep[]
  groups: ChangeColumn[]
}
