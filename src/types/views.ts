import type { Pipeline, PipelineStatus } from './pipelines'
import type { ViewPresentation } from './plugin_presentation'
import type { SectionPresentation } from './presentation'
import type { ProviderFreshness, ProviderSummary } from './providers'

export type ViewRef = string
export type ViewKind = 'all' | 'pinned' | 'hidden' | 'custom' | 'config'
export type ViewSortKey = 'name' | 'last_run' | 'status' | 'organization' | 'provider' | 'manual'
export type SortDirection = 'asc' | 'desc'
export type SectionMode = 'repository' | 'organization' | 'provider' | 'status' | 'none'
export type ViewLayout = 'table' | 'cards'

export interface ViewDisplay {
  sort: ViewSortKey
  direction: SortDirection
  sections: SectionMode
  layout: ViewLayout
}

export interface ViewRules {
  provider_ids: number[]
  repositories: string[]
  name_contains: string | null
  statuses: PipelineStatus[]
}

export interface ViewCapabilities {
  rename: boolean
  delete: boolean
  edit_rules: boolean
  edit_members: boolean
  edit_display: boolean
  reorder: boolean
}

export interface PipelineView {
  id: ViewRef
  kind: ViewKind
  name: string
  rules: ViewRules
  display: ViewDisplay
  capabilities: ViewCapabilities
  count: number
  parent: ViewRef | null
}

export interface ViewMembership {
  view: ViewRef
  source: 'manual' | 'rule' | 'sub_view'
}

export interface ViewPipeline extends Pipeline {
  organization: string
  pinned: boolean
  hidden: boolean
  memberships: ViewMembership[]
}

export interface ViewSection {
  id: string
  kind: SectionMode | 'view'
  title: string
  organization: string | null
  provider_ids: number[]
  status: PipelineStatus | null
  status_pipeline_id: string | null
  last_run: string | null
  pending_fetch: boolean
  pipelines: ViewPipeline[]
  presentation: SectionPresentation
  freshness: ProviderFreshness
}

export interface PipelineViewPage {
  view: PipelineView
  pinned: ViewPipeline[]
  pinned_blocks: PinnedBlock[]
  sections: ViewSection[]
  total: number
  hidden_count: number
  filters_active: boolean
  section_count: number
  facets: { organizations: string[]; provider_ids: number[] }
  presentation: ViewPresentation
}

export type PinKind = 'view' | 'repository'

export interface PinnedItem {
  kind: PinKind
  ref: string
  name: string
}

export interface PinnedBlock {
  kind: 'repository'
  ref: string
  name: string
  pipelines: ViewPipeline[]
}

export type DateRangeValue = 'today' | '24h' | '7d' | '30d' | '60d' | '90d'

export interface PipelineViewQuery {
  view: ViewRef
  provider_id?: number | undefined
  search?: string | undefined
  statuses?: PipelineStatus[] | undefined
  organization?: string | undefined
  date_range?: DateRangeValue | undefined
  show_hidden: boolean
  utc_offset_minutes: number
}

export interface CreateViewInput {
  name: string
  rules?: ViewRules
  display?: ViewDisplay
  members?: string[]
  parent?: ViewRef | null
}

export interface ViewPatch {
  name?: string
  rules?: ViewRules
  display?: ViewDisplay
  members?: string[]
  parent?: ViewRef | null
}

export interface ViewMember {
  pipeline_id: string
  name: string | null
  repository: string | null
  provider_id: number
  available: boolean
}

export interface LegacyMarks {
  pinned: string[]
  hidden: string[]
}

export interface LegacyImportResult {
  imported: number
  skipped: number
}

export interface ViewSelectionEntry {
  view: PipelineView
  depth: number
  selected_count: number
  direct_count: number
  included_by_rule: boolean
  included_by_sub_view: boolean
}

export interface ViewMembershipChange {
  pipeline_ids: string[]
  member: boolean
}

export interface ViewEditorOptions {
  eligible_parents: PipelineView[]
  can_add_sub_view: boolean
}

export interface PipelineNavigation {
  views: PipelineView[]
  pins: PinnedItem[]
  providers: ProviderSummary[]
  total_pipelines: number
}
