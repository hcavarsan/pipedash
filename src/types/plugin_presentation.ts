import type { PipelineStatus } from './pipelines'
import type { ActionCopy, Notice, PipelineTerminology, ViewCopy, WizardCopy } from './presentation'

export interface StatusOption {
  value: PipelineStatus
  label: string
}

export interface PluginPresentation {
  terminology: PipelineTerminology
  status_options: StatusOption[]
  wizard: WizardCopy
  view: ViewCopy
  trigger: ActionCopy
  rerun: ActionCopy
  history_note: string | null
  empty_history_description: string
  run_details_title: string
  progress_title: string
  archived_notice: Notice | null
  force_cancel_confirmation: string
}

export interface ViewPresentation {
  terminology: PipelineTerminology
  status_options: StatusOption[]
  copy: ViewCopy
}
