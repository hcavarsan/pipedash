import { useQueries } from '@tanstack/react-query'
import { type Dispatch, useMemo } from 'react'

import { DEBOUNCE_DELAYS } from '../../../../constants/intervals'
import { useDebounce } from '../../../../hooks/useDebounce'
import { service } from '../../../../services/service'
import type { ConfigField, PluginMetadata } from '../../../../types/plugins'
import { type FieldAttributes, type FieldInputKind, fieldAttributes } from '../../../../utils/formFields'

import type { FormAction } from './types'
import { type FieldStep, STEP_SECTIONS } from './validation'

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
  configValues: Record<string, string>
  fieldErrors: Record<string, string>
  dispatch: Dispatch<FormAction>
  validateKey: (key: string) => Promise<void>
  step: FieldStep
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
  configValues,
  fieldErrors,
  dispatch,
  validateKey,
  step,
}: UseCredentialFieldsParams): UseCredentialFieldsResult {
  const debouncedConfigValues = useDebounce(configValues, DEBOUNCE_DELAYS.FILTER)
  const schemaFields = selectedPlugin?.config_schema.fields ?? NO_FIELDS
  const fields = useMemo(
    () => STEP_SECTIONS[step].flatMap((section) => schemaFields.filter((field) => field.section === section)),
    [schemaFields, step]
  )
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
  }

  const handleConfigBlur = (field: ConfigField): void => {
    void validateKey(field.key)
  }

  const handleNameChange = (name: string): void => {
    dispatch({ type: 'SET_PROVIDER_NAME', name })
  }

  const handleNameBlur = (): void => {
    void validateKey('providerName')
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
