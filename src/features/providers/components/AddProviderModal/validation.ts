import type { ConfigField, ConfigSection } from '../../../../types/plugins'

import type { Step } from './types'

export function mapProviderFieldErrors(
  errors: Record<string, string>,
  configFields: Pick<ConfigField, 'key'>[]
): Record<string, string> {
  const fields: Record<string, string> = {}

  for (const [key, message] of Object.entries(errors)) {
    const field = key === 'name' ? 'providerName' : key.replace(/^config\./, '')

    if (field === 'providerName' || configFields.some((candidate) => candidate.key === field)) {
      fields[field] = message
    }
  }

  return fields
}

export type FieldStep = Exclude<Step, 'pipelines'>

export const STEP_SECTIONS: Record<FieldStep, readonly ConfigSection[]> = {
  provider: ['settings'],
  credentials: ['connection', 'authentication'],
}

export const stepOfField = (key: string, fields: ConfigField[]): FieldStep =>
  key === 'providerName' || fields.find((field) => field.key === key)?.section === 'settings'
    ? 'provider'
    : 'credentials'

export const stepWithErrors = (errors: Record<string, string>, fields: ConfigField[]): FieldStep =>
  Object.keys(errors).some((key) => stepOfField(key, fields) === 'provider') ? 'provider' : 'credentials'
