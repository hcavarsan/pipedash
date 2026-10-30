import type { AuthMode, ConfigField, SignInStatus } from '../../../../types/domain'
import { SIGN_IN_KEY, TOKEN_KEY } from '../../lib/credentials'

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

export const activeConfigFields = (
  fields: ConfigField[],
  authMode: AuthMode,
  authModes: Record<string, AuthMode> | undefined
): ConfigField[] =>
  fields.filter(
    (field) => !(authMode === 'sign_in' && field.key === TOKEN_KEY) && (authModes?.[field.key] ?? authMode) === authMode
  )

export const activeConfigValues = (
  configValues: Record<string, string>,
  authMode: AuthMode,
  authModes: Record<string, AuthMode> | undefined
): Record<string, string> =>
  Object.fromEntries(Object.entries(configValues).filter(([key]) => (authModes?.[key] ?? authMode) === authMode))

export function validateSignIn(
  authMode: AuthMode,
  configValues: Record<string, string>,
  status: SignInStatus | undefined
): string | null {
  if (authMode !== 'sign_in') {
    return null
  }

  if (!configValues[SIGN_IN_KEY]) {
    return 'Sign in to continue'
  }

  return status?.state === 'completed' ? null : 'Finish signing in to continue'
}

interface CredentialsDraft {
  providerName: string
  fields: ConfigField[]
  authMode: AuthMode
  authModes: Record<string, AuthMode> | undefined
  configValues: Record<string, string>
  signInStatus: SignInStatus | undefined
}

export function validateProviderCredentials({
  providerName,
  fields,
  authMode,
  authModes,
  configValues,
  signInStatus,
}: CredentialsDraft): Record<string, string> {
  const errors: Record<string, string> = {}
  const nameError = validateProviderName(providerName)

  if (nameError) {
    errors.providerName = nameError
  }

  for (const field of activeConfigFields(fields, authMode, authModes)) {
    const fieldError = validateConfigField(field, configValues[field.key] || '')

    if (fieldError) {
      errors[field.key] = fieldError
    }
  }

  const signInError = validateSignIn(authMode, configValues, signInStatus)

  if (signInError) {
    errors[SIGN_IN_KEY] = signInError
  }

  return errors
}
