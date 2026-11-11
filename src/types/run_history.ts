import type { PipelineRun, PipelineStatus } from './pipelines'
import type { DateRangeValue, SortDirection } from './views'

export interface RunHistoryQuery {
  page: number
  page_size: number
  search?: string | undefined
  statuses: PipelineStatus[]
  branch?: string | undefined
  actor?: string | undefined
  date_range?: DateRangeValue | undefined
  utc_offset_minutes: number
  sort?: { column: string; direction: SortDirection } | undefined
  scan_depth?: number | undefined
}

export interface RunHistoryScanNotice {
  title: string
  message: string
  action_label: string
}

export interface RunHistoryPage {
  runs: PipelineRun[]
  total_count: number
  has_more: boolean
  can_scan_deeper: boolean
  is_complete: boolean
  scan_notice: RunHistoryScanNotice | null
  sort: { column: string | null; direction: SortDirection }
  page: number
  page_size: number
  total_pages: number
  branches: string[]
  actors: string[]
}
