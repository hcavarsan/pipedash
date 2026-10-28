import type { Pipeline, PipelineRun } from '../types'

import type { RerunSummaryField } from './rerunSummary'

export type TargetPipeline = Pick<Pipeline, 'name' | 'repository' | 'branch' | 'provider_type' | 'metadata'>
type TargetRun = Pick<PipelineRun, 'branch' | 'inputs'>

const text = (value: unknown) => (typeof value === 'string' ? value.trim() : '')

const joinUnique = (values: string[]) => [...new Set(values.filter(Boolean))].join(', ')

export const deployTarget = ({ provider_type, metadata }: Pick<Pipeline, 'provider_type' | 'metadata'>) => {
  if (provider_type === 'argocd') {
    const value = [text(metadata?.destination_cluster), text(metadata?.destination_namespace)]
      .filter(Boolean)
      .join(' / ')

    return value ? { label: 'Destination', value } : null
  }

  const namespace = text(metadata?.namespace)

  return namespace ? { label: 'Namespace', value: namespace } : null
}

const runEnvironment = (run: Pick<PipelineRun, 'inputs'>) =>
  text(Object.entries(run.inputs ?? {}).find(([key]) => /(^|\.)env(ironment)?$/i.test(key))?.[1])

export const actionTarget = (pipeline: TargetPipeline, runs: TargetRun[] = []): RerunSummaryField[] =>
  [
    { label: 'Pipeline', value: pipeline.name },
    { label: 'Repository', value: pipeline.repository },
    {
      label: 'Branch',
      value: joinUnique(runs.length > 0 ? runs.map((run) => run.branch ?? '') : [pipeline.branch ?? '']),
    },
    { label: 'Environment', value: joinUnique(runs.map(runEnvironment)) },
    deployTarget(pipeline),
  ].flatMap((field) => (field?.value ? [field] : []))
