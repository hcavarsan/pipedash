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
  sort_column: string
  direction: SortDirection
}

export interface RunHistoryPage {
  runs: PipelineRun[]
  total_count: number
  has_more: boolean
  is_complete: boolean
  page: number
  page_size: number
  total_pages: number
  branches: string[]
  actors: string[]
}
