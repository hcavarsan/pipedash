import { STATUS_OPTIONS } from '../theme/status'
import type { Pipeline } from '../types/domain'

export interface PipelineLabels {
  item: string
  items: string
  group: string
  groups: string
  groupTitle: string
  sourceTitle: string
  history: string
  trigger: string
  run: string
  runs: string
}

const pipelineLabels: PipelineLabels = {
  item: 'pipeline',
  items: 'pipelines',
  group: 'organization',
  groups: 'organizations',
  groupTitle: 'Organization',
  sourceTitle: 'Repository',
  history: 'Run history',
  trigger: 'Trigger',
  run: 'run',
  runs: 'runs',
}

const fluxLabels: PipelineLabels = {
  item: 'resource',
  items: 'resources',
  group: 'namespace',
  groups: 'namespaces',
  groupTitle: 'Namespace',
  sourceTitle: 'Source',
  history: 'Reconciliation history',
  trigger: 'Reconcile',
  run: 'reconciliation',
  runs: 'reconciliations',
}

export const getPipelineLabels = (providerType?: string): PipelineLabels =>
  providerType === 'fluxcd' ? fluxLabels : pipelineLabels

export const getStatusLabel = (resource: Pick<Pipeline, 'metadata' | 'overdue_run'>): string | undefined => {
  if (resource.overdue_run) {
    return 'Running longer than usual'
  }

  const label = resource.metadata?.status_label

  if (typeof label !== 'string') {
    return undefined
  }

  return resource.metadata?.archived === true ? `${label} (cached)` : label
}

const fluxStatusOptions = [
  { value: 'success', label: 'Ready' },
  { value: 'failed', label: 'Not ready' },
  { value: 'running', label: 'Reconciling' },
  { value: 'pending', label: 'Pending' },
  { value: 'skipped', label: 'Suspended' },
]

export const getStatusOptions = (providerType?: string): { value: string; label: string }[] =>
  providerType === 'fluxcd' ? fluxStatusOptions : STATUS_OPTIONS

const isDynamicGitHubWorkflow = (pipeline: Pick<Pipeline, 'provider_type' | 'workflow_file'>) =>
  pipeline.provider_type === 'github' && pipeline.workflow_file?.startsWith('dynamic/') === true

export const canRerunInPlace = (pipeline: Pick<Pipeline, 'provider_type'>): boolean =>
  pipeline.provider_type === 'github'

export const getReconcileBlockReason = (pipeline: Pick<Pipeline, 'metadata'>): string | undefined => {
  if (pipeline.metadata?.missing === true) {
    return 'This resource was not found. Update the selection or recreate it in Flux.'
  }
  if (pipeline.metadata?.suspended === true) {
    return 'This resource is suspended. Resume it in Flux before requesting reconciliation.'
  }

  return undefined
}

export const getTriggerBlockReason = (
  pipeline: Pick<Pipeline, 'metadata' | 'provider_type' | 'workflow_file'>
): string | undefined =>
  isDynamicGitHubWorkflow(pipeline)
    ? 'GitHub starts this built-in workflow itself. It has no workflow file, so only its existing runs can be rerun.'
    : getReconcileBlockReason(pipeline)
