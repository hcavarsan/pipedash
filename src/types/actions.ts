import type { DisplayField } from './presentation'

export interface ActionAvailability {
  allowed: boolean
  reason: string | null
}

export type RerunStrategy = 'in_place' | 'replay_inputs' | 'reconcile_current'

export type ActionIcon = 'play' | 'refresh'

export interface PipelineActionState {
  trigger: ActionAvailability
  cancel: ActionAvailability
  force_cancel: ActionAvailability
  enable: ActionAvailability
  rerun_strategy: RerunStrategy
  trigger_icon: ActionIcon
  target: DisplayField | null
}

export interface RunActionState {
  cancel: ActionAvailability
  force_cancel: ActionAvailability
  rerun: ActionAvailability
  rerun_failed: ActionAvailability
  review: ActionAvailability
  watch: boolean
}

export interface RunPace {
  live: boolean
  typical_duration_seconds: number | null
  past_usual: boolean
}
