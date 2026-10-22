import {
  Box,
  Loader,
  MultiSelect,
  NumberInput,
  PasswordInput,
  ScrollArea,
  Select,
  Stack,
  Switch,
  Text,
  Textarea,
  TextInput,
} from '@mantine/core'
import { useQueries } from '@tanstack/react-query'
import { type Dispatch, useMemo } from 'react'

import { DEBOUNCE_DELAYS } from '../../../constants/intervals'
import { useDebounce } from '../../../hooks/useDebounce'
import { service } from '../../../services'
import type { ConfigField, PluginMetadata } from '../../../types'

import type { FormAction, FormState } from './types'

interface CredentialsStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
  plugins: PluginMetadata[]
  pluginsLoading: boolean
  editMode: boolean
  isMobile: boolean
}

export function CredentialsStep({
  state,
  dispatch,
  plugins,
  pluginsLoading,
  editMode,
  isMobile,
}: CredentialsStepProps) {
  const { selectedPlugin, providerName, configValues, fieldErrors, submitting } = state

  const configValuesKey = useMemo(() => JSON.stringify(configValues), [configValues])
  const debouncedConfigValuesKey = useDebounce(configValuesKey, DEBOUNCE_DELAYS.FILTER)

  const dynamicFields =
    selectedPlugin?.config_schema.fields.filter(
      (field) => (field.field_type === 'Select' || field.field_type === 'MultiSelect') && !field.options?.length
    ) ?? []
  const optionQueries = useQueries({
    queries: dynamicFields.map((field) => ({
      queryKey: ['providerFieldOptions', selectedPlugin?.provider_type, field.key, debouncedConfigValuesKey],
      queryFn: () =>
        service.getProviderFieldOptions(
          selectedPlugin?.provider_type ?? '',
          field.key,
          JSON.parse(debouncedConfigValuesKey) as Record<string, string>
        ),
      enabled: !!selectedPlugin,
      retry: false,
    })),
  })

  const handlePluginSelect = (providerType: string | null) => {
    if (!providerType) {
      dispatch({ type: 'CLEAR_PLUGIN' })

      return
    }

    const plugin = plugins.find((p) => p.provider_type === providerType)

    if (plugin) {
      dispatch({ type: 'SELECT_PLUGIN', plugin, isEditMode: editMode })
    }
  }

  const handleConfigChange = (key: string, value: string) => {
    dispatch({ type: 'UPDATE_CONFIG', key, value })
  }

  const renderConfigField = (field: ConfigField) => {
    const value = configValues[field.key] || ''
    const fieldOptions = optionQueries[dynamicFields.findIndex((candidate) => candidate.key === field.key)]
    const loadingOptions = !!fieldOptions && (fieldOptions.isFetching || debouncedConfigValuesKey !== configValuesKey)
    const commonProps = {
      label: field.label,
      description: field.description ?? undefined,
      value,
      error: fieldErrors[field.key] || fieldOptions?.error?.message,
      withAsterisk: field.required,
      onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement> | string) => {
        const newValue = typeof e === 'string' ? e : e?.currentTarget?.value

        handleConfigChange(field.key, newValue)
      },
    }

    switch (field.field_type) {
      case 'TextArea':
        return <Textarea key={field.key} {...commonProps} rows={4} />
      case 'Password':
        return <PasswordInput key={field.key} {...commonProps} />
      case 'Number':
        return (
          <NumberInput
            key={field.key}
            {...commonProps}
            onChange={(val) => handleConfigChange(field.key, String(val || ''))}
          />
        )
      case 'Select':
        return (
          <Select
            key={field.key}
            {...commonProps}
            data={fieldOptions?.data ?? field.options ?? []}
            rightSection={loadingOptions ? <Loader size="xs" aria-label={`Loading ${field.label}`} /> : undefined}
            disabled={submitting || loadingOptions}
            onChange={(val) => handleConfigChange(field.key, val || '')}
            searchable
            clearable
          />
        )
      case 'MultiSelect':
        return (
          <MultiSelect
            key={field.key}
            {...commonProps}
            value={value ? value.split(',').filter(Boolean) : []}
            data={fieldOptions?.data ?? field.options ?? []}
            rightSection={loadingOptions ? <Loader size="xs" aria-label={`Loading ${field.label}`} /> : undefined}
            disabled={submitting || loadingOptions}
            onChange={(values) => handleConfigChange(field.key, values.join(','))}
            searchable
            clearable
          />
        )
      case 'Boolean':
        return (
          <Switch
            key={field.key}
            label={field.label}
            description={field.description ?? undefined}
            checked={value === 'true'}
            error={fieldErrors[field.key]}
            onChange={(e) => handleConfigChange(field.key, String(e.currentTarget.checked))}
          />
        )
      default:
        return <TextInput key={field.key} {...commonProps} />
    }
  }

  return (
    <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      <ScrollArea style={{ flex: 1 }} type="auto">
        <Stack gap={isMobile ? 'xs' : 'md'} px="lg" pb="lg" pt="xs">
          <Select
            label="Provider type"
            placeholder="Select a provider type"
            data={plugins.map((p) => ({
              value: p.provider_type,
              label: p.name,
            }))}
            value={selectedPlugin?.provider_type || null}
            onChange={handlePluginSelect}
            required
            disabled={submitting || editMode}
          />

          {selectedPlugin ? (
            <Stack gap={isMobile ? 'xs' : 'md'}>
              <Text size="sm" c="dimmed">
                {selectedPlugin.description}
              </Text>

              <TextInput
                label="Provider name"
                description="A friendly name to identify this provider"
                placeholder={
                  selectedPlugin.provider_type === 'fluxcd' ? 'e.g. FluxCD (production)' : 'e.g. GitHub (work)'
                }
                value={providerName}
                onChange={(e) => dispatch({ type: 'SET_PROVIDER_NAME', name: e.currentTarget.value })}
                disabled={submitting}
                error={fieldErrors.providerName}
                withAsterisk
              />

              {selectedPlugin.config_schema.fields.map((field) => renderConfigField(field))}
            </Stack>
          ) : (
            !pluginsLoading && (
              <Text size="sm" c="dimmed" ta="center" py="xl">
                Select a provider type to continue
              </Text>
            )
          )}
        </Stack>
      </ScrollArea>
    </Box>
  )
}
