import type { ActionAvailability, PipelineActionState, RunActionState, RunPace } from './actions'
import type { ActionCopy, DisplayField, RunDisplay } from './presentation'

export type PipelineStatus = 'success' | 'failed' | 'degraded' | 'running' | 'pending' | 'cancelled' | 'skipped'

export interface ResourceState {
  label: string
  message?: string
}

export type ReviewDecision = 'approve' | 'reject'

export interface AvailablePipeline {
  id: string
  name: string
  description: string | null
  organization: string | null
  repository: string | null
}

export interface Organization {
  id: string
  name: string
  description: string | null
}

export interface Pipeline {
  id: string
  provider_id: number
  provider_type: string
  name: string
  status: PipelineStatus
  last_run: string | null
  last_updated: string
  repository: string
  branch: string | null
  workflow_file: string | null
  metadata?: Record<string, unknown>
  overdue_run?: string | null
  typical_duration_seconds?: number
  resource_state?: ResourceState
  actions: PipelineActionState
}

export interface PipelineRun {
  id: string
  pipeline_id: string
  run_number: number
  status: PipelineStatus
  started_at: string
  concluded_at: string | null
  duration_seconds: number | null
  logs_url: string
  commit_sha: string | null
  commit_message: string | null
  branch: string | null
  actor: string | null
  inputs?: Record<string, unknown>
  metadata?: Record<string, unknown>
  actions: RunActionState
  display: RunDisplay
  pace: RunPace
  [key: string]: unknown
}

export interface PaginatedResponse<T> {
  items: T[]
  page: number
  page_size: number
  total_count: number
  total_pages: number
  has_more: boolean
}

export type PaginatedAvailablePipelines = PaginatedResponse<AvailablePipeline>

export interface TriggerParams {
  workflow_id: string
  inputs?: Record<string, unknown> | undefined
}

interface WorkflowParameterBase {
  name: string
  label: string | null
  description: string | null
  required: boolean
}

export type WorkflowParameter = WorkflowParameterBase &
  (
    | { type: 'string'; default?: string | null; options?: never }
    | { type: 'boolean'; default?: boolean | null; options?: never }
    | { type: 'choice'; default?: string | null; options: string[] }
    | { type: 'number'; default?: number | null; options?: never }
  )

export interface TriggerResult {
  run_id: string | null
  run_number: number | null
}

export interface TriggerDraft {
  parameters: WorkflowParameter[]
  values: Record<string, unknown>
  fields: DisplayField[]
  errors: Record<string, string>
  availability: ActionAvailability
  copy: ActionCopy
}
