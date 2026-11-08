import type { ContextMenuItem } from '../components/common/ContextMenu'
import type { StatusKey } from '../theme/status'
import type { Pipeline, PipelineStatus } from './pipelines'
import type { PipelineTerminology } from './presentation'
import type { ProviderFreshness } from './providers'
import type { PinKind, SectionMode, ViewPipeline } from './views'

export type WorkflowMenuItem = Extract<ContextMenuItem, { label: string }>

interface BaseTableRow {
  id: string
  name: string
  depth?: number | undefined
  organization?: string | undefined
  providerIds: Set<number>
  providerLabel?: string | undefined
  lastRun?: string | null | undefined
}

export interface SectionRow extends BaseTableRow {
  type: 'section'
  kind: SectionMode | 'view'
  expanded: boolean
  pipelineCount: number
  hasPendingFetch: boolean
  status: StatusKey | null
  pipelines: ViewPipeline[]
  freshness: ProviderFreshness
  organizationLabel: string
  countLabel: string
}

export interface WorkflowRow extends BaseTableRow {
  type: 'workflow'
  pipeline: ViewPipeline
  status: PipelineStatus
}

export type TableRow = SectionRow | WorkflowRow

export type SectionMenuTarget = Pick<SectionRow, 'type' | 'id' | 'name' | 'kind' | 'pipelines'>

export interface PipelineActions {
  onTogglePin: (p: ViewPipeline) => void
  onToggleHidden: (p: ViewPipeline) => void
  onAddToView: (p: ViewPipeline) => void
  onRemoveFromView?: ((p: ViewPipeline) => void) | undefined
  removeLabel?: string | undefined
  onTrigger: (p: Pipeline) => void
  onEnable: (p: Pipeline) => void
  onViewMetrics?: ((p: Pipeline) => void) | undefined
  onViewHistory: (p: Pipeline) => void
  onOpenView: (viewId: string) => void
  onRefreshProviders: (providerIds: number[]) => void
  isRefreshing: (providerIds: number[]) => boolean
  pinItem: (kind: PinKind, ref: string) => WorkflowMenuItem
  cancelItem: (label: string, pipelines: ViewPipeline[]) => WorkflowMenuItem | null
  notifyItems: (pipeline: ViewPipeline) => WorkflowMenuItem[]
  terminology: (providerType: string) => PipelineTerminology | undefined
}

export interface PipelineSelection {
  selected: Set<string>
  onToggle: (pipeline: ViewPipeline) => void
}

export interface PinToggle {
  pinned: boolean
  label: string
  onToggle: () => void
}
