import type { Pipeline, PipelineRun } from '../types'

import { deployTarget } from './actionTarget'

export type RerunMode = 'all' | 'failed'

export interface RerunSummaryField {
  label: string
  value: string
}

export const canRerunFailedOnly = (run: Pick<PipelineRun, 'metadata'>): boolean =>
  run.metadata?.can_rerun_failed_jobs === true

export const buildRerunSummary = (
  run: PipelineRun,
  {
    pipeline,
    providerName,
  }: {
    pipeline: Pick<Pipeline, 'name' | 'repository' | 'provider_type' | 'metadata'> | undefined
    providerName: string
  }
): { fields: RerunSummaryField[]; inputs: RerunSummaryField[] } => {
  const message = run.commit_message?.split('\n')[0]?.trim()
  const commit = [run.commit_sha?.slice(0, 7), message].filter(Boolean).join(' ')

  const fields = [
    { label: 'Pipeline', value: pipeline?.name ?? '' },
    { label: 'Repository', value: pipeline?.repository ?? '' },
    { label: 'Provider', value: providerName },
    { label: 'Run', value: `#${run.run_number}` },
    { label: 'Branch', value: run.branch ?? '' },
    { label: 'Commit', value: commit },
    (pipeline && deployTarget(pipeline)) ?? { label: '', value: '' },
  ].filter(({ value }) => value)

  const inputs = Object.entries(run.inputs ?? {})
    .filter(
      ([key, value]) =>
        value !== null &&
        value !== undefined &&
        value !== '' &&
        !((key === 'ref' || key === 'branch') && value === run.branch)
    )
    .map(([label, value]) => ({ label, value: typeof value === 'object' ? JSON.stringify(value) : String(value) }))

  return { fields, inputs }
}
