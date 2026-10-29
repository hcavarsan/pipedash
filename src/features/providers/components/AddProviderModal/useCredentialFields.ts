import { useQueries } from '@tanstack/react-query'
import type { Dispatch } from 'react'

import { DEBOUNCE_DELAYS } from '../../../../constants/intervals'
import { useDebounce } from '../../../../hooks/useDebounce'
import { service } from '../../../../services/service'
import type { ConfigField, PluginMetadata } from '../../../../types/domain'
import { type FieldAttributes, type FieldInputKind, fieldAttributes } from '../../../../utils/formFields'

import type { FormAction } from './types'
import { validateConfigField, validateProviderName } from './validation'

const INPUT_KINDS: Partial<Record<ConfigField['field_type'], FieldInputKind>> = {
  Password: 'password',
  Number: 'number',
  TextArea: 'textarea',
}

export interface CredentialFieldModel {
  value: string
  options: string[]
  loadingOptions: boolean
  error: string | undefined
  attributes: FieldAttributes
}

interface UseCredentialFieldsParams {
  selectedPlugin: PluginMetadata | null
  providerName: string
  configValues: Record<string, string>
  fieldErrors: Record<string, string>
  dispatch: Dispatch<FormAction>
}

interface UseCredentialFieldsResult {
  fields: ConfigField[]
  lastTextKey: string | undefined
  fieldStateFor: (field: ConfigField) => CredentialFieldModel
  handleConfigChange: (field: ConfigField, value: string) => void
  handleConfigBlur: (field: ConfigField) => void
  handleNameChange: (name: string) => void
  handleNameBlur: () => void
}

const NO_FIELDS: ConfigField[] = []

export function useCredentialFields({
  selectedPlugin,
  providerName,
  configValues,
  fieldErrors,
  dispatch,
}: UseCredentialFieldsParams): UseCredentialFieldsResult {
  const debouncedConfigValues = useDebounce(configValues, DEBOUNCE_DELAYS.FILTER)
  const fields = selectedPlugin?.config_schema.fields ?? NO_FIELDS
  const lastTextKey = fields.findLast((field) => field.field_type !== 'Boolean')?.key
  const dynamicFields = fields.filter(
    (field) => (field.field_type === 'Select' || field.field_type === 'MultiSelect') && !field.options?.length
  )
  const optionQueries = useQueries({
    queries: dynamicFields.map((field) => ({
      queryKey: ['providerFieldOptions', selectedPlugin?.provider_type, field.key, debouncedConfigValues],
      queryFn: () =>
        service.getProviderFieldOptions(selectedPlugin?.provider_type ?? '', field.key, debouncedConfigValues),
      enabled: !!selectedPlugin,
      retry: false,
    })),
  })

  const handleConfigChange = (field: ConfigField, value: string): void => {
    dispatch({ type: 'UPDATE_CONFIG', key: field.key, value })

    if (fieldErrors[field.key]) {
      const error = validateConfigField(field, value)

      if (error) {
        dispatch({ type: 'SET_FIELD_ERROR', key: field.key, error })
      }
    }
  }

  const handleConfigBlur = (field: ConfigField): void => {
    const error = validateConfigField(field, configValues[field.key] || '')

    if (error) {
      dispatch({ type: 'SET_FIELD_ERROR', key: field.key, error })
    } else if (fieldErrors[field.key]) {
      dispatch({ type: 'CLEAR_FIELD_ERROR', key: field.key })
    }
  }

  const handleNameChange = (name: string): void => {
    dispatch({ type: 'SET_PROVIDER_NAME', name })

    if (fieldErrors.providerName) {
      const error = validateProviderName(name)

      if (error) {
        dispatch({ type: 'SET_FIELD_ERROR', key: 'providerName', error })
      }
    }
  }

  const handleNameBlur = (): void => {
    const error = validateProviderName(providerName)

    if (error) {
      dispatch({ type: 'SET_FIELD_ERROR', key: 'providerName', error })
    } else if (fieldErrors.providerName) {
      dispatch({ type: 'CLEAR_FIELD_ERROR', key: 'providerName' })
    }
  }

  const fieldStateFor = (field: ConfigField): CredentialFieldModel => {
    const fieldOptions = optionQueries[dynamicFields.findIndex((candidate) => candidate.key === field.key)]

    return {
      value: configValues[field.key] || '',
      options: fieldOptions?.data ?? field.options ?? [],
      loadingOptions: !!fieldOptions && (fieldOptions.isFetching || debouncedConfigValues !== configValues),
      error: fieldErrors[field.key] || fieldOptions?.error?.message,
      attributes: fieldAttributes({
        name: field.key,
        label: field.label,
        kind: INPUT_KINDS[field.field_type] ?? 'text',
        defaultValue: field.default_value,
        last: field.key === lastTextKey,
      }),
    }
  }

  return { fields, lastTextKey, fieldStateFor, handleConfigChange, handleConfigBlur, handleNameChange, handleNameBlur }
}
