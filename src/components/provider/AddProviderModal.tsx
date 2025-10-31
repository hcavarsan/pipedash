import { useEffect, useState } from 'react'

import {
  Alert,
  Box,
  Button,
  Card,
  Checkbox as MantineCheckbox,
  Group,
  Loader,
  NumberInput,
  Paper,
  PasswordInput,
  ScrollArea,
  Select,
  SimpleGrid,
  Stack,
  Stepper,
  Table,
  Text,
  Textarea,
  TextInput,
} from '@mantine/core'
import { IconAlertCircle, IconCheck, IconKey, IconList } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { usePlugins } from '../../contexts/PluginContext'
import { tauriService } from '../../services/tauri'
import type { AvailablePipeline, ConfigField, PluginMetadata, ProviderConfig } from '../../types'
import { StandardModal } from '../common/StandardModal'

interface AddProviderModalProps {
  opened: boolean;
  onClose: () => void;
  onAdd?: (config: ProviderConfig) => Promise<void>;
  onUpdate?: (id: number, config: ProviderConfig) => Promise<void>;
  editMode?: boolean;
  existingProvider?: ProviderConfig & { id: number };
}

type Step = 'credentials' | 'pipelines';

export const AddProviderModal = ({
  opened,
  onClose,
  onAdd,
  onUpdate,
  editMode = false,
  existingProvider,
}: AddProviderModalProps) => {
  const isMobile = useIsMobile()
  const { plugins, loading: pluginsLoading } = usePlugins()
  const [step, setStep] = useState<Step>('credentials')
  const [selectedPlugin, setSelectedPlugin] = useState<PluginMetadata | null>(null)
  const [providerName, setProviderName] = useState('')
  const [token, setToken] = useState('')
  const [configValues, setConfigValues] = useState<Record<string, string>>({})
  const [availablePipelines, setAvailablePipelines] = useState<AvailablePipeline[]>([])
  const [selectedPipelines, setSelectedPipelines] = useState<Set<string>>(new Set())
  const [loadingPipelines, setLoadingPipelines] = useState(false)
  const [organizationFilter, setOrganizationFilter] = useState<string>('')
  const [repositoryFilter, setRepositoryFilter] = useState<string>('')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({})

  useEffect(() => {
    if (!opened) {
      // Reset form when modal closes
      setStep('credentials')
      setSelectedPlugin(null)
      setProviderName('')
      setToken('')
      setConfigValues({})
      setAvailablePipelines([])
      setSelectedPipelines(new Set())
      setError(null)
      setSuccess(false)
    } else if (opened && editMode && existingProvider) {
      const plugin = plugins.find((p) => p.provider_type === existingProvider.provider_type)


      if (plugin) {
        setSelectedPlugin(plugin)
        setProviderName(existingProvider.name)
        setToken(existingProvider.token)
        setConfigValues(existingProvider.config)
      }
    }
  }, [opened, editMode, existingProvider, plugins])

  const handlePluginSelect = (providerType: string | null) => {
    if (!providerType) {
      setSelectedPlugin(null)
      setConfigValues({})
      setProviderName('')

return
    }

    const plugin = plugins.find((p) => p.provider_type === providerType)


    if (plugin) {
      setSelectedPlugin(plugin)

      if (!editMode) {
        setProviderName(plugin.name)
      }

      // Initialize config values with default values
      const initialConfig: Record<string, string> = {}


      plugin.config_schema.fields.forEach((field) => {
        if (field.default_value) {
          initialConfig[field.key] = field.default_value
        }
      })
      setConfigValues(initialConfig)
    }
  }

  const handleConfigChange = (key: string, value: string) => {
    setConfigValues((prev) => ({
      ...prev,
      [key]: value,
    }))
  }

  const renderConfigField = (field: ConfigField) => {
    const value = configValues[field.key] || ''
    const commonProps = {
      key: field.key,
      label: field.label,
      description: field.description || undefined,
      required: field.required,
      value,
      error: fieldErrors[field.key],
      onChange: (e: any) => {
        const newValue = e?.currentTarget?.value ?? e


        handleConfigChange(field.key, newValue)
        if (fieldErrors[field.key]) {
          const newErrors = { ...fieldErrors }

          delete newErrors[field.key]
          setFieldErrors(newErrors)
        }
      },
    }

    switch (field.field_type) {
      case 'TextArea':
        return <Textarea {...commonProps} rows={4} />
      case 'Password':
        return <PasswordInput {...commonProps} />
      case 'Number':
        return (
          <NumberInput
            {...commonProps}
            onChange={(val) => handleConfigChange(field.key, String(val || ''))}
          />
        )
      case 'Select':
        return (
          <Select
            {...commonProps}
            data={field.options || []}
            onChange={(val) => handleConfigChange(field.key, val || '')}
          />
        )
      case 'Checkbox':
        return (
          <MantineCheckbox
            {...commonProps}
            checked={value === 'true'}
            onChange={(e) =>
              handleConfigChange(field.key, String(e.currentTarget.checked))
            }
          />
        )
      case 'Text':
      default:
        return <TextInput {...commonProps} />
    }
  }

  const validateField = (field: ConfigField, value: string): string | null => {
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

  const validateCredentials = (): boolean => {
    const errors: Record<string, string> = {}

    if (!providerName.trim()) {
      errors.providerName = 'Provider name is required'
    }

    if (!token.trim()) {
      errors.token = 'API token is required'
    }

    if (!selectedPlugin) {
      setError('Please select a provider type')

return false
    }

    for (const field of selectedPlugin.config_schema.fields) {
      const fieldError = validateField(field, configValues[field.key] || '')


      if (fieldError) {
        errors[field.key] = fieldError
      }
    }

    setFieldErrors(errors)

    if (Object.keys(errors).length > 0) {
      setError('Please fix the errors below')

return false
    }

    return true
  }

  const handleNext = async () => {
    if (!validateCredentials() || !selectedPlugin) {
return
}

    try {
      setLoadingPipelines(true)
      setError(null)

      const pipelines = await tauriService.previewProviderPipelines(
        selectedPlugin.provider_type,
        token,
        configValues
      )

      setAvailablePipelines(pipelines)

      if (editMode && existingProvider) {
        const existingPipelineIds = new Set<string>()

        const selectedItems = existingProvider.config.selected_items || ''


        if (selectedItems) {
          selectedItems.split(',').forEach((id) => {
            const trimmed = id.trim()


            if (trimmed) {
              existingPipelineIds.add(trimmed)
            }
          })
        }

        setSelectedPipelines(existingPipelineIds)
      }

      setStep('pipelines')
    } catch (err: any) {
      setError(err?.error || err?.message || 'Failed to fetch available pipelines')
    } finally {
      setLoadingPipelines(false)
    }
  }

  const handlePipelineToggle = (pipelineId: string) => {
    setSelectedPipelines((prev) => {
      const newSet = new Set(prev)


      if (newSet.has(pipelineId)) {
        newSet.delete(pipelineId)
      } else {
        newSet.add(pipelineId)
      }

return newSet
    })
  }

  const handleSelectAll = () => {
    if (selectedPipelines.size === availablePipelines.length) {
      setSelectedPipelines(new Set())
    } else {
      setSelectedPipelines(new Set(availablePipelines.map((p) => p.id)))
    }
  }

  const uniqueOrganizations = Array.from(
    new Set(availablePipelines.map((p) => p.organization).filter((org): org is string => !!org))
  ).sort()

  const uniqueRepositories = Array.from(
    new Set(availablePipelines.map((p) => p.repository).filter((repo): repo is string => !!repo))
  ).sort()

  const filteredPipelines = availablePipelines.filter((pipeline) => {
    const matchesOrg = !organizationFilter || pipeline.organization === organizationFilter
    const matchesRepo = !repositoryFilter || pipeline.repository === repositoryFilter



return matchesOrg && matchesRepo
  })

  const handleSubmit = async () => {
    if (!selectedPlugin || selectedPipelines.size === 0) {
      setError('Please select at least one pipeline')

return
    }

    try {
      setSubmitting(true)
      setError(null)

      const finalConfig = { ...configValues }

      finalConfig.selected_items = Array.from(selectedPipelines).join(',')

      const providerConfig: ProviderConfig = {
        name: providerName,
        provider_type: selectedPlugin.provider_type,
        token,
        config: finalConfig,
        refresh_interval: 30,
      }

      if (editMode && existingProvider && onUpdate) {
        await onUpdate(existingProvider.id, providerConfig)
      } else if (onAdd) {
        await onAdd(providerConfig)
      }

      setSuccess(true)

      setTimeout(() => {
        onClose()
      }, 1500)
    } catch (err: any) {
      setError(err?.error || err?.message || `Failed to ${editMode ? 'update' : 'add'} provider`)
    } finally {
      setSubmitting(false)
    }
  }

  const activeStepIndex = step === 'credentials' ? 0 : 1

  const renderMobilePipelineCards = () => {
    return (
      <Stack gap="xs">
        {filteredPipelines.map((pipeline) => {
          const isSelected = selectedPipelines.has(pipeline.id)



return (
            <Card
              key={pipeline.id}
              padding="xs"
              withBorder
              style={{
                cursor: 'pointer',
                backgroundColor: isSelected ? 'var(--mantine-color-blue-light)' : undefined,
              }}
              onClick={() => handlePipelineToggle(pipeline.id)}
            >
              <Stack gap={4}>
                <Group justify="space-between" wrap="nowrap">
                  <Group gap={8} wrap="nowrap" style={{ flex: 1, overflow: 'hidden' }}>
                    <MantineCheckbox
                      checked={isSelected}
                      onChange={() => handlePipelineToggle(pipeline.id)}
                      style={{ flexShrink: 0 }}
                    />
                    <Text size="sm" fw={500} truncate style={{ flex: 1 }}>
                      {pipeline.name}
                    </Text>
                  </Group>
                </Group>

                <Group gap="xs" wrap="nowrap" align="flex-start">
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Text size="xs" c="dimmed">Org</Text>
                    <Text size="xs" truncate>{pipeline.organization || '—'}</Text>
                  </Box>
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Text size="xs" c="dimmed">Repo</Text>
                    <Text size="xs" truncate>{pipeline.repository || '—'}</Text>
                  </Box>
                </Group>

                {pipeline.description && (
                  <Box>
                    <Text size="xs" c="dimmed" lineClamp={1}>{pipeline.description}</Text>
                  </Box>
                )}
              </Stack>
            </Card>
          )
        })}
      </Stack>
    )
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title={editMode ? 'Edit Provider' : 'Add Provider'}
      loading={submitting}
    >
      <Stack gap={isMobile ? 'xs' : 'md'} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <Stepper active={activeStepIndex} size={isMobile ? 'xs' : 'sm'} iconSize={isMobile ? 32 : 42}>
          <Stepper.Step
            label={isMobile ? undefined : 'Credentials'}
            description={isMobile ? undefined : 'Enter your API credentials'}
            icon={<IconKey size={18} />}
            completedIcon={<IconCheck size={18} />}
          />
          <Stepper.Step
            label={isMobile ? undefined : 'Select Pipelines'}
            description={isMobile ? undefined : 'Choose pipelines to monitor'}
            icon={<IconList size={18} />}
            completedIcon={<IconCheck size={18} />}
            loading={loadingPipelines}
          />
        </Stepper>

        {pluginsLoading ? (
          <Group justify="center" py="xl">
            <Loader size="sm" />
            <Text size="sm" c="dimmed">
              Loading available providers...
            </Text>
          </Group>
        ) : (
          <>
            {error && (
              <Alert icon={<IconAlertCircle size={16} />} color="red" title="Error">
                {error}
              </Alert>
            )}

            {success && (
              <Alert icon={<IconCheck size={16} />} color="green" title="Success">
                Provider {editMode ? 'updated' : 'added'} successfully!
              </Alert>
            )}

            {step === 'credentials' && (
              <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
                <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
                  <Stack gap={isMobile ? 'xs' : 'md'} style={{ flex: 1, overflow: 'auto' }}>
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
                      disabled={submitting || success || editMode}
                    />

                    {selectedPlugin ? (
                      <Stack gap={isMobile ? 'xs' : 'md'}>
                        <Text size="sm" c="dimmed">
                          {selectedPlugin.description}
                        </Text>

                        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing={isMobile ? 'xs' : 'md'}>
                          <TextInput
                            label="Provider Name"
                            placeholder="My Provider"
                            value={providerName}
                            onChange={(e) => {
                              setProviderName(e.currentTarget.value)
                              if (fieldErrors.providerName) {
                                const newErrors = { ...fieldErrors }

                                delete newErrors.providerName
                                setFieldErrors(newErrors)
                              }
                            }}
                            required
                            disabled={submitting || success}
                            description="A friendly name to identify this provider"
                            error={fieldErrors.providerName}
                          />

                          <PasswordInput
                            label="API Token"
                            placeholder="Enter your API token"
                            value={token}
                            onChange={(e) => {
                              setToken(e.currentTarget.value)
                              if (fieldErrors.token) {
                                const newErrors = { ...fieldErrors }

                                delete newErrors.token
                                setFieldErrors(newErrors)
                              }
                            }}
                            required
                            disabled={submitting || success}
                            description="Your API token for authentication"
                            error={fieldErrors.token}
                          />

                          {selectedPlugin.config_schema.fields.map((field) =>
                            renderConfigField(field)
                          )}
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
                </Box>

                <Box
                  style={{
                    borderTop: '1px solid var(--mantine-color-default-border)',
                    paddingTop: isMobile ? 8 : 12,
                    marginTop: 0,
                    flexShrink: 0,
                  }}
                >
                  <Group justify="flex-end" gap={isMobile ? 'xs' : 'sm'}>
                    <Button
                      variant="subtle"
                      size={isMobile ? 'sm' : 'md'}
                      onClick={onClose}
                      disabled={submitting || success}
                    >
                      Cancel
                    </Button>
                    <Button
                      onClick={handleNext}
                      size={isMobile ? 'sm' : 'md'}
                      loading={loadingPipelines}
                      disabled={!selectedPlugin || success || !providerName.trim() || !token.trim()}
                    >
                      {isMobile ? 'Next' : 'Next: Select Pipelines'}
                    </Button>
                  </Group>
                </Box>
              </Box>
            )}

            {step === 'pipelines' && (
              <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
                <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
                  <Stack gap={isMobile ? 'xs' : 'sm'} style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
                    <Paper p={{ base: 'xs', sm: 'md' }} withBorder style={{ flexShrink: 0 }}>
                    <Stack gap="xs">
                      <Group justify="space-between" wrap="nowrap">
                        <Box style={{ flex: 1, minWidth: 0 }}>
                          <Text size="sm" fw={500} truncate>
                            {filteredPipelines.length} of {availablePipelines.length}
                          </Text>
                          <Text size="xs" c="dimmed">
                            {selectedPipelines.size} selected
                          </Text>
                        </Box>
                        <Button
                          size="xs"
                          variant="subtle"
                          onClick={handleSelectAll}
                        >
                          {isMobile ? (selectedPipelines.size === availablePipelines.length ? 'Clear' : 'All') : (selectedPipelines.size === availablePipelines.length ? 'Deselect All' : 'Select All')}
                        </Button>
                      </Group>
                      <Group grow>
                        <Select
                          placeholder={isMobile ? 'Org...' : 'Filter by organization...'}
                          size="xs"
                          value={organizationFilter}
                          onChange={(value) => setOrganizationFilter(value || '')}
                          data={uniqueOrganizations}
                          clearable
                          searchable
                        />
                        <Select
                          placeholder={isMobile ? 'Repo...' : 'Filter by repository...'}
                          size="xs"
                          value={repositoryFilter}
                          onChange={(value) => setRepositoryFilter(value || '')}
                          data={uniqueRepositories}
                          clearable
                          searchable
                        />
                      </Group>
                    </Stack>
                  </Paper>

                  {isMobile ? (
                    <Box style={{ flex: 1, overflow: 'auto' }}>
                      {renderMobilePipelineCards()}
                    </Box>
                  ) : (
                    <ScrollArea h={400}>
                      <Table highlightOnHover>
                        <Table.Thead>
                          <Table.Tr>
                            <Table.Th style={{ width: 40 }}></Table.Th>
                            <Table.Th>Name</Table.Th>
                            <Table.Th>Organization</Table.Th>
                            <Table.Th>Repository</Table.Th>
                            <Table.Th>Description</Table.Th>
                          </Table.Tr>
                        </Table.Thead>
                        <Table.Tbody>
                          {filteredPipelines.map((pipeline) => (
                            <Table.Tr
                              key={pipeline.id}
                              onClick={() => handlePipelineToggle(pipeline.id)}
                              style={{ cursor: 'pointer' }}
                            >
                              <Table.Td>
                                <MantineCheckbox
                                  checked={selectedPipelines.has(pipeline.id)}
                                  onChange={() => handlePipelineToggle(pipeline.id)}
                                />
                              </Table.Td>
                              <Table.Td>
                                <Text size="sm" fw={500}>
                                  {pipeline.name}
                                </Text>
                              </Table.Td>
                              <Table.Td>
                                <Text size="sm">
                                  {pipeline.organization || '—'}
                                </Text>
                              </Table.Td>
                              <Table.Td>
                                <Text size="sm">
                                  {pipeline.repository || '—'}
                                </Text>
                              </Table.Td>
                              <Table.Td>
                                <Text size="sm" c="dimmed">
                                  {pipeline.description || '—'}
                                </Text>
                              </Table.Td>
                            </Table.Tr>
                          ))}
                        </Table.Tbody>
                      </Table>
                    </ScrollArea>
                  )}
                </Stack>
                </Box>

                <Box
                  style={{
                    borderTop: '1px solid var(--mantine-color-default-border)',
                    paddingTop: isMobile ? 8 : 12,
                    marginTop: isMobile ? 8 : 12,
                    flexShrink: 0,
                  }}
                >
                  <Group justify="space-between">
                    <Button
                      variant="subtle"
                      size={isMobile ? 'sm' : 'md'}
                      onClick={() => setStep('credentials')}
                      disabled={submitting || success}
                    >
                      Back
                    </Button>
                    <Group gap={isMobile ? 'xs' : 'sm'}>
                      <Button
                        variant="subtle"
                        size={isMobile ? 'sm' : 'md'}
                        onClick={onClose}
                        disabled={submitting || success}
                      >
                        Cancel
                      </Button>
                      <Button
                        onClick={handleSubmit}
                        size={isMobile ? 'sm' : 'md'}
                        loading={submitting}
                        disabled={success || selectedPipelines.size === 0}
                      >
                        {success ? 'Done!' : editMode ? (isMobile ? 'Update' : 'Update Provider') : (isMobile ? 'Add' : 'Add Provider')}
                      </Button>
                    </Group>
                  </Group>
                </Box>
              </Box>
            )}
          </>
        )}
      </Stack>
    </StandardModal>
  )
}
