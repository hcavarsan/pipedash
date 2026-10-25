import type { ConfigField } from '../../../types'

type ValidatedField = Pick<ConfigField, 'label' | 'required' | 'validation_regex' | 'validation_message'>

export const PROVIDER_NAME_ERROR = 'Provider name is required'

export function validateConfigField(field: ValidatedField, value: string): string | null {
  if (field.required && !value?.trim()) {
    return `${field.label} is required`
  }

  if (field.validation_regex && value) {
    const regex = new RegExp(field.validation_regex)

    if (!regex.test(value)) {
      return field.validation_message || `Invalid format for ${field.label}`
    }
  }

  return null
}

export const validateProviderName = (name: string): string | null => (name.trim() ? null : PROVIDER_NAME_ERROR)

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
