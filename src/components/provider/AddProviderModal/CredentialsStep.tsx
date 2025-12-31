import { Dispatch, useEffect, useMemo, useRef } from 'react'

import {
  Box,
  Checkbox as MantineCheckbox,
  Group,
  NumberInput,
  PasswordInput,
  ScrollArea,
  Select,
  SimpleGrid,
  Stack,
  Text,
  Textarea,
  TextInput,
  Tooltip,
} from '@mantine/core'
import { IconInfoCircle } from '@tabler/icons-react'

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
  const { selectedPlugin, providerName, configValues, dynamicOptions, fieldErrors, submitting } = state

  const configValuesKey = useMemo(() => JSON.stringify(configValues), [configValues])
  const debouncedConfigValuesKey = useDebounce(configValuesKey, DEBOUNCE_DELAYS.FILTER)

  const loadedOptionsRef = useRef<{
    pluginType: string | null
    configKey: string
  }>({ pluginType: null, configKey: '' })

  useEffect(() => {
    if (!selectedPlugin) {
return
}

    const currentKey = `${selectedPlugin.provider_type}:${debouncedConfigValuesKey}`


    if (
      loadedOptionsRef.current.pluginType === selectedPlugin.provider_type &&
      loadedOptionsRef.current.configKey === currentKey
    ) {
      return
    }

    let isMounted = true

    const loadDynamicOptions = async () => {
      const newDynamicOptions: Record<string, string[]> = {}
      const parsedConfig = JSON.parse(debouncedConfigValuesKey) as Record<string, string>

      for (const field of selectedPlugin.config_schema.fields) {
        if (field.field_type === 'Select' && (!field.options || field.options.length === 0)) {
          try {
            const options = await service.getProviderFieldOptions(
              selectedPlugin.provider_type,
              field.key,
              parsedConfig
            )

            if (options.length > 0 && isMounted) {
              newDynamicOptions[field.key] = options
            }
          } catch (err) {
            console.error(`Failed to load options for ${field.key}:`, err)
          }
        }
      }

      if (isMounted && Object.keys(newDynamicOptions).length > 0) {
        dispatch({ type: 'SET_DYNAMIC_OPTIONS', options: newDynamicOptions })
        loadedOptionsRef.current = {
          pluginType: selectedPlugin.provider_type,
          configKey: currentKey,
        }
      }
    }

    loadDynamicOptions()

    return () => {
      isMounted = false
    }
  }, [selectedPlugin, debouncedConfigValuesKey, dispatch])

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

  const renderFieldLabel = (label: string, description: string | null, required: boolean) => {
    return (
      <Group gap={4} wrap="nowrap">
        <Text size="sm" fw={500}>
          {label}
          {required && <Text component="span" c="red"> *</Text>}
        </Text>
        {description && (
          <Tooltip label={description} multiline w={300} withArrow>
            <Box style={{ display: 'flex', alignItems: 'center', cursor: 'help' }}>
              <IconInfoCircle size={14} style={{ opacity: 0.6 }} />
            </Box>
          </Tooltip>
        )}
      </Group>
    )
  }

  const renderConfigField = (field: ConfigField) => {
    const value = configValues[field.key] || ''
    const commonProps = {
      label: renderFieldLabel(field.label, field.description, field.required),
      value,
      error: fieldErrors[field.key],
      withAsterisk: false,
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
            data={dynamicOptions[field.key] ?? field.options ?? []}
            onChange={(val) => handleConfigChange(field.key, val || '')}
            searchable
            clearable
          />
        )
      case 'Checkbox':
        return (
          <MantineCheckbox
            key={field.key}
            label={renderFieldLabel(field.label, field.description, field.required)}
            checked={value === 'true'}
            error={fieldErrors[field.key]}
            onChange={(e) => handleConfigChange(field.key, String(e.currentTarget.checked))}
          />
        )
      case 'Text':
      default:
        return <TextInput key={field.key} {...commonProps} />
    }
  }

  return (
    <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      <ScrollArea style={{ flex: 1 }} type="auto">
        <Stack gap={isMobile ? 'xs' : 'md'} p="md">
          <Select
            label="Provider Type"
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

              <SimpleGrid cols={{ base: 1, sm: 2 }} spacing={isMobile ? 'xs' : 'md'}>
                <TextInput
                  label={renderFieldLabel('Provider Name', 'A friendly name to identify this provider', true)}
                  placeholder="My Provider"
                  value={providerName}
                  onChange={(e) => dispatch({ type: 'SET_PROVIDER_NAME', name: e.currentTarget.value })}
                  disabled={submitting}
                  error={fieldErrors.providerName}
                  withAsterisk={false}
                />

                {selectedPlugin.config_schema.fields.map((field) => renderConfigField(field))}
              </SimpleGrid>
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
