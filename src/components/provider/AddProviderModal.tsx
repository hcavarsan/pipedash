import { useEffect, useMemo, useState } from 'react'

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
  Tooltip,
} from '@mantine/core'
import { IconAlertCircle, IconCheck, IconInfoCircle, IconKey, IconList, IconSearch } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { usePlugins } from '../../contexts/PluginContext'
import { tauriService } from '../../services/tauri'
import type { AvailablePipeline, ConfigField, FeatureAvailability, Organization, PaginatedAvailablePipelines, PermissionStatus, PluginMetadata, ProviderConfig } from '../../types'
import { THEME_COLORS, THEME_TYPOGRAPHY } from '../../utils/dynamicRenderers'
import { StandardModal } from '../common/StandardModal'

import { PermissionCheckButton } from './PermissionCheckButton'
import { PermissionCheckModal } from './PermissionCheckModal'

interface AddProviderModalProps {
  opened: boolean;
  onClose: () => void;
  onAdd?: (config: ProviderConfig) => Promise<void>;
  onUpdate?: (id: number, config: ProviderConfig) => Promise<void>;
  editMode?: boolean;
  existingProvider?: ProviderConfig & { id: number };
}

type Step = 'credentials' | 'pipelines';

// eslint-disable-next-line max-lines-per-function, max-statements
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
  const [configValues, setConfigValues] = useState<Record<string, string>>({})
  const [availableOrganizations, setAvailableOrganizations] = useState<Organization[]>([])
  const [selectedOrganization, setSelectedOrganization] = useState<string>('')
  const [pipelineCache, setPipelineCache] = useState<Map<string, PaginatedAvailablePipelines>>(new Map())
  const [allPipelines, setAllPipelines] = useState<PaginatedAvailablePipelines | null>(null)
  const [availablePipelines, setAvailablePipelines] = useState<AvailablePipeline[]>([])
  const [selectedPipelines, setSelectedPipelines] = useState<Set<string>>(new Set())
  const [loadingOrganizations, setLoadingOrganizations] = useState(false)
  const [loadingPipelines, setLoadingPipelines] = useState(false)
  const [loadingMore, setLoadingMore] = useState(false)
  const [searchingPipelines, setSearchingPipelines] = useState(false)
  const [currentPage, setCurrentPage] = useState(1)
  const [pipelinesPerPage] = useState(100)
  const [repositorySearch, setRepositorySearch] = useState<string>('')
  const [searchDebounceTimer, setSearchDebounceTimer] = useState<NodeJS.Timeout | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({})
  const [dynamicOptions, setDynamicOptions] = useState<Record<string, string[]>>({})

  // Permission check state
  const [permissionModalOpen, setPermissionModalOpen] = useState(false)
  const [permissionStatus, setPermissionStatus] = useState<PermissionStatus | null>(null)
  const [permissionFeatures, setPermissionFeatures] = useState<FeatureAvailability[]>([])
  const [checkingPermissions, setCheckingPermissions] = useState(false)
  const [permissionError, setPermissionError] = useState<string | null>(null)

  useEffect(() => {
    if (!opened) {
      setStep('credentials')
      setSelectedPlugin(null)
      setProviderName('')
      setConfigValues({})
      setAvailableOrganizations([])
      setSelectedOrganization('')
      setPipelineCache(new Map())
      setAllPipelines(null)
      setAvailablePipelines([])
      setSelectedPipelines(new Set())
      setRepositorySearch('')
      setSearchingPipelines(false)
      setSearchDebounceTimer((prev) => {
        if (prev) {
clearTimeout(prev)
}

return null
      })
      setError(null)
      setFieldErrors({})
      setDynamicOptions({})
      setCurrentPage(1)
      setLoadingMore(false)
    } else if (opened && editMode && existingProvider) {
      const plugin = plugins.find((p) => p.provider_type === existingProvider.provider_type)


      if (plugin) {
        setSelectedPlugin(plugin)
        setProviderName(existingProvider.name)
        const initialConfig = { ...existingProvider.config }


        if (existingProvider.token) {
          initialConfig.token = existingProvider.token
        }
        setConfigValues(initialConfig)
      }
    }
  }, [opened, editMode, existingProvider, plugins])

  const handlePluginSelect = (providerType: string | null) => {
    if (!providerType) {
      setSelectedPlugin(null)
      setConfigValues({})
      setProviderName('')
      setDynamicOptions({})

return
    }

    const plugin = plugins.find((p) => p.provider_type === providerType)


    if (plugin) {
      setSelectedPlugin(plugin)
      if (!editMode) {
        setProviderName(plugin.name)
      }

      const initialConfig: Record<string, string> = {}


      plugin.config_schema.fields.forEach((field) => {
        if (field.default_value) {
          const defaultVal = typeof field.default_value === 'string'
            ? field.default_value
            : String(field.default_value)


          initialConfig[field.key] = defaultVal
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

  const configValuesKey = useMemo(() => JSON.stringify(configValues), [configValues])

  useEffect(() => {
    if (!selectedPlugin) {
return
}

    const loadDynamicOptions = async () => {
      const newDynamicOptions: Record<string, string[]> = {}

      for (const field of selectedPlugin.config_schema.fields) {
        if (field.field_type === 'Select' && (!field.options || field.options.length === 0)) {
          try {
            const options = await tauriService.getProviderFieldOptions(
              selectedPlugin.provider_type,
              field.key,
              configValues
            )


            if (options.length > 0) {
              newDynamicOptions[field.key] = options
            }
          } catch (err) {
            console.error(`Failed to load options for ${field.key}:`, err)
          }
        }
      }

      if (Object.keys(newDynamicOptions).length > 0) {
        setDynamicOptions(newDynamicOptions)
      }
    }

    loadDynamicOptions()
  }, [selectedPlugin, configValuesKey, configValues])

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
            data={dynamicOptions[field.key] || field.options || []}
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
            onChange={(e) =>
              handleConfigChange(field.key, String(e.currentTarget.checked))
            }
          />
        )
      case 'Text':
      default:
        return <TextInput key={field.key} {...commonProps} />
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
      setLoadingOrganizations(true)
      setError(null)

      const orgs = await tauriService.fetchProviderOrganizations(
        selectedPlugin.provider_type,
        configValues
      )

      setAvailableOrganizations(orgs)

      if (orgs.length === 1) {
        setSelectedOrganization(orgs[0].id)
        await loadPipelinesForOrg(orgs[0].id)
      } else if (orgs.length === 0) {
        const response = await tauriService.previewProviderPipelines(
          selectedPlugin.provider_type,
          configValues,
          undefined,
          undefined,
          1,
          pipelinesPerPage
        )


        setAllPipelines(response)
        setAvailablePipelines(response.items)
        setCurrentPage(1)
      }

      setStep('pipelines')
    } catch (err: any) {
      setError(err?.error || err?.message || 'Failed to fetch organizations')
    } finally {
      setLoadingOrganizations(false)
    }
  }

  const loadPipelinesForOrg = async (orgId: string) => {
    if (pipelineCache.has(orgId)) {
      const cached = pipelineCache.get(orgId)!


      setAllPipelines(cached)
      setAvailablePipelines(cached.items)
      
return
    }

    setLoadingPipelines(true)
    try {
      const response = await tauriService.previewProviderPipelines(
        selectedPlugin!.provider_type,
        configValues,
        orgId,
        undefined,
        1,
        pipelinesPerPage
      )

      setPipelineCache((prev) => new Map(prev).set(orgId, response))
      setAllPipelines(response)
      setAvailablePipelines(response.items)
      setCurrentPage(1)

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
    } catch (err: any) {
      setError(err?.error || err?.message || 'Failed to fetch pipelines')
    } finally {
      setLoadingPipelines(false)
    }
  }

  const handleOrganizationSelect = (org: string | null) => {
    if (!org || !selectedPlugin) {
      return
    }

    setSelectedOrganization(org)
    setRepositorySearch('')
    loadPipelinesForOrg(org)
  }

  const handleSearchChange = (value: string) => {
    setRepositorySearch(value)

    if (searchDebounceTimer) {
      clearTimeout(searchDebounceTimer)
    }

    if (value.length > 0 && value.length < 2) {
      return
    }

    const timer = setTimeout(async () => {
      if (!selectedOrganization || !selectedPlugin) {
        return
      }

      setSearchingPipelines(true)
      try {
        const response = await tauriService.previewProviderPipelines(
          selectedPlugin.provider_type,
          configValues,
          selectedOrganization,
          value || undefined,
          1,
          pipelinesPerPage
        )

        setAllPipelines(response)
        setAvailablePipelines(response.items)
        setCurrentPage(1)
      } catch (err: any) {
        setError(err?.error || err?.message || 'Failed to search pipelines')
      } finally {
        setSearchingPipelines(false)
      }
    }, 300)

    setSearchDebounceTimer(timer)
  }

  const handleCheckPermissions = async () => {
    if (!selectedPlugin) {
      return
    }

    setCheckingPermissions(true)
    setPermissionModalOpen(true)
    setPermissionError(null)
    setPermissionStatus(null)
    setPermissionFeatures([])

    try {
      const config = { ...configValues }
      const result = await tauriService.checkProviderPermissions(
        selectedPlugin.provider_type,
        config
      )

      setPermissionStatus(result.permission_status)
      setPermissionFeatures(result.features)
    } catch (err: any) {
      setPermissionError(err?.error || err?.message || 'Failed to check permissions')
    } finally {
      setCheckingPermissions(false)
    }
  }

  const handleLoadMore = async () => {
    if (!selectedPlugin || !allPipelines || !allPipelines.has_more || loadingMore) {
      return
    }

    try {
      setLoadingMore(true)
      const nextPage = currentPage + 1

      const paginatedResponse = await tauriService.previewProviderPipelines(
        selectedPlugin.provider_type,
        configValues,
        selectedOrganization || undefined,
        repositorySearch || undefined,
        nextPage,
        pipelinesPerPage
      )

      const combinedItems = [...allPipelines.items, ...paginatedResponse.items]

      setAllPipelines({
        items: combinedItems,
        page: nextPage,
        page_size: paginatedResponse.page_size,
        total_count: allPipelines.total_count,
        total_pages: allPipelines.total_pages,
        has_more: paginatedResponse.has_more,
      })
      setAvailablePipelines(combinedItems)
      setCurrentPage(nextPage)
    } catch (err: any) {
      setError(err?.error || err?.message || 'Failed to load more pipelines')
    } finally {
      setLoadingMore(false)
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
    if (selectedPipelines.size === filteredPipelines.length) {
      setSelectedPipelines(new Set())
    } else {
      setSelectedPipelines(new Set(filteredPipelines.map((p) => p.id)))
    }
  }

  const filteredPipelines = useMemo(() => {
    if (!repositorySearch.trim()) {
      return availablePipelines
    }
    const searchLower = repositorySearch.toLowerCase()


    
return availablePipelines.filter((pipeline) => {
      return pipeline.repository?.toLowerCase().includes(searchLower)
    })
  }, [availablePipelines, repositorySearch])

  const handleSubmit = async () => {
    if (!selectedPlugin || selectedPipelines.size === 0) {
      setError('Please select at least one pipeline')

return
    }

    try {
      setSubmitting(true)
      setError(null)

      const finalConfig = { ...configValues }
      const token = finalConfig.token || ''


      delete finalConfig.token

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

      onClose()
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
                    <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Org</Text>
                    <Text size={THEME_TYPOGRAPHY.FIELD_VALUE_SMALL.size} c={THEME_COLORS.VALUE_TEXT} truncate>{pipeline.organization || '—'}</Text>
                  </Box>
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Repo</Text>
                    <Text size={THEME_TYPOGRAPHY.FIELD_VALUE_SMALL.size} c={THEME_COLORS.VALUE_TEXT} truncate>{pipeline.repository || '—'}</Text>
                  </Box>
                </Group>

                {pipeline.description && (
                  <Box>
                    <Text size={THEME_TYPOGRAPHY.FIELD_VALUE_SMALL.size} c={THEME_COLORS.DIMMED} lineClamp={1}>{pipeline.description}</Text>
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
            loading={loadingOrganizations || loadingPipelines}
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

            {step === 'credentials' && (
              <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
                <ScrollArea style={{ flex: 1 }} type="auto">
                  <Stack gap={isMobile ? 'xs' : 'md'} pb="md">
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
                            onChange={(e) => {
                              setProviderName(e.currentTarget.value)
                              if (fieldErrors.providerName) {
                                const newErrors = { ...fieldErrors }

                                delete newErrors.providerName
                                setFieldErrors(newErrors)
                              }
                            }}
                            disabled={submitting}
                            error={fieldErrors.providerName}
                            withAsterisk={false}
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
                </ScrollArea>

                <Box
                  style={{
                    borderTop: '1px solid var(--mantine-color-default-border)',
                    paddingTop: isMobile ? 8 : 12,
                    marginTop: isMobile ? 8 : 12,
                    flexShrink: 0,
                  }}
                >
                  <Group justify="space-between" gap="xs">
                    {/* Left side: Permission check button */}
                    <Group gap="xs">
                      {selectedPlugin && selectedPlugin.required_permissions.length > 0 && (
                        <PermissionCheckButton
                          onClick={handleCheckPermissions}
                          disabled={!configValues.token || submitting}
                          loading={checkingPermissions}
                        />
                      )}
                    </Group>

                    {/* Right side: Cancel and Next buttons */}
                    <Group gap="xs">
                      <Button
                        variant="light"
                        size="sm"
                        onClick={onClose}
                        disabled={submitting}
                      >
                        Cancel
                      </Button>
                      {editMode && (
                        <Button
                          variant="filled"
                          color="blue"
                          size="sm"
                          onClick={async () => {
                            if (!validateCredentials() || !selectedPlugin) {
                              return
                            }

                            try {
                              setSubmitting(true)
                              setError(null)

                              const finalConfig = { ...configValues }
                              const token = finalConfig.token || ''

                              delete finalConfig.token

                              const providerConfig: ProviderConfig = {
                                name: providerName,
                                provider_type: selectedPlugin.provider_type,
                                token,
                                config: finalConfig,
                                refresh_interval: 30,
                              }

                              if (existingProvider && onUpdate) {
                                await onUpdate(existingProvider.id, providerConfig)
                              }

                              onClose()
                            } catch (err: any) {
                              setError(err?.error || err?.message || 'Failed to update provider')
                            } finally {
                              setSubmitting(false)
                            }
                          }}
                          loading={submitting}
                          disabled={!selectedPlugin || !providerName.trim()}
                        >
                          Save
                        </Button>
                      )}
                      <Button
                        variant="light"
                        color="blue"
                        size="sm"
                        onClick={handleNext}
                        loading={loadingOrganizations}
                        disabled={!selectedPlugin || !providerName.trim()}
                      >
                        {isMobile ? 'Next' : 'Next: Select Pipelines'}
                      </Button>
                    </Group>
                  </Group>
                </Box>
              </Box>
            )}

            {step === 'pipelines' && (
              <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
                <Stack gap="xs" style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
                  <Paper p="sm" withBorder style={{ flexShrink: 0 }}>
                    {loadingOrganizations ? (
                      <Box py="lg">
                        <Stack align="center" gap="xs">
                          <Loader size="md" />
                          <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED}>Loading organizations...</Text>
                        </Stack>
                      </Box>
                    ) : (
                      <Group gap="sm" wrap="nowrap" align="flex-start">
                        {availableOrganizations.length > 1 && (
                          <Select
                            placeholder="Select organization"
                            value={selectedOrganization}
                            onChange={handleOrganizationSelect}
                            data={availableOrganizations.map(org => ({ value: org.id, label: org.name }))}
                            searchable
                            disabled={loadingPipelines}
                            rightSection={loadingPipelines ? <Loader size="xs" /> : undefined}
                            style={{ flex: 1 }}
                            styles={{
                              input: {
                                height: 36,
                                fontSize: '0.875rem',
                              },
                            }}
                          />
                        )}
                        <TextInput
                          placeholder="Search repositories..."
                          value={repositorySearch}
                          onChange={(e) => handleSearchChange(e.currentTarget.value)}
                          leftSection={searchingPipelines ? <Loader size={16} /> : <IconSearch size={16} />}
                          disabled={!selectedOrganization}
                          style={{ flex: 1 }}
                          styles={{
                            input: {
                              height: 36,
                              fontSize: '0.875rem',
                            },
                          }}
                        />
                      </Group>
                    )}
                  </Paper>

                  {loadingOrganizations ? (
                    <Box style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                      <Stack align="center" gap="xs">
                        <Loader size="md" />
                        <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED}>Fetching organizations...</Text>
                      </Stack>
                    </Box>
                  ) : loadingPipelines ? (
                    <Box style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                      <Stack align="center" gap="xs">
                        <Loader size="md" />
                        <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED}>Loading pipelines...</Text>
                      </Stack>
                    </Box>
                  ) : !selectedOrganization ? (
                    <Box style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                      <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED}>
                        {availableOrganizations.length > 1 ? 'Select organization above' : 'No organizations found'}
                      </Text>
                    </Box>
                  ) : isMobile ? (
                    <ScrollArea style={{ flex: 1 }} type="auto">
                      {renderMobilePipelineCards()}
                    </ScrollArea>
                  ) : (
                    <>
                      <ScrollArea style={{ flex: 1 }} type="auto">
                        <Table
                          highlightOnHover
                          verticalSpacing="xs"
                          styles={{
                            tr: {
                              height: 44,
                            },
                          }}
                        >
                          <Table.Thead>
                            <Table.Tr>
                              <Table.Th style={{ width: 50 }}>
                                {filteredPipelines.length > 0 && (
                                  <MantineCheckbox
                                    checked={selectedPipelines.size === filteredPipelines.length && filteredPipelines.length > 0}
                                    indeterminate={selectedPipelines.size > 0 && selectedPipelines.size < filteredPipelines.length}
                                    onChange={handleSelectAll}
                                  />
                                )}
                              </Table.Th>
                              <Table.Th style={{ width: '25%' }}>Name</Table.Th>
                              <Table.Th style={{ width: '15%' }}>Organization</Table.Th>
                              <Table.Th style={{ width: '20%' }}>Repository</Table.Th>
                              <Table.Th style={{ width: '40%' }}>Description</Table.Th>
                            </Table.Tr>
                          </Table.Thead>
                          <Table.Tbody>
                            {filteredPipelines.map((pipeline) => (
                              <Table.Tr
                                key={pipeline.id}
                                onClick={() => handlePipelineToggle(pipeline.id)}
                                style={{ cursor: 'pointer', height: 44 }}
                              >
                                <Table.Td onClick={(e) => e.stopPropagation()}>
                                  <MantineCheckbox
                                    checked={selectedPipelines.has(pipeline.id)}
                                    onChange={() => handlePipelineToggle(pipeline.id)}
                                  />
                                </Table.Td>
                                <Table.Td style={{ maxWidth: 0 }}>
                                  <Tooltip label={pipeline.name} openDelay={500}>
                                    <Text size="sm" fw={500} truncate="end">
                                      {pipeline.name}
                                    </Text>
                                  </Tooltip>
                                </Table.Td>
                                <Table.Td style={{ maxWidth: 0 }}>
                                  <Tooltip label={pipeline.organization || '—'} openDelay={500}>
                                    <Text size="sm" truncate="end">
                                      {pipeline.organization || '—'}
                                    </Text>
                                  </Tooltip>
                                </Table.Td>
                                <Table.Td style={{ maxWidth: 0 }}>
                                  <Tooltip label={pipeline.repository || '—'} openDelay={500}>
                                    <Text size="sm" truncate="end">
                                      {pipeline.repository || '—'}
                                    </Text>
                                  </Tooltip>
                                </Table.Td>
                                <Table.Td style={{ maxWidth: 0 }}>
                                  <Tooltip label={pipeline.description || '—'} openDelay={500}>
                                    <Text size="sm" c="dimmed" truncate="end">
                                      {pipeline.description || '—'}
                                    </Text>
                                  </Tooltip>
                                </Table.Td>
                              </Table.Tr>
                            ))}
                          </Table.Tbody>
                        </Table>
                      </ScrollArea>

                      {allPipelines && selectedOrganization && allPipelines.has_more && (
                        <Group justify="center" pt={4}>
                          <Button
                            size="xs"
                            variant="filled"
                            color="dark.5"
                            onClick={handleLoadMore}
                            loading={loadingMore}
                            disabled={loadingMore}
                          >
                            Load More
                          </Button>
                        </Group>
                      )}
                    </>
                  )}
                </Stack>

                <Box
                  style={{
                    borderTop: '1px solid var(--mantine-color-default-border)',
                    paddingTop: isMobile ? 8 : 12,
                    marginTop: isMobile ? 8 : 12,
                    flexShrink: 0,
                  }}
                >
                  <Group justify="space-between">
                    <Group gap="sm">
                      <Button
                        variant="light"
                        size="sm"
                        onClick={() => setStep('credentials')}
                        disabled={submitting}
                      >
                        Back
                      </Button>
                      {selectedOrganization && selectedPipelines.size > 0 && (
                        <Text size="sm" c="dimmed">
                          {selectedPipelines.size} selected
                        </Text>
                      )}
                    </Group>
                    <Group gap="xs">
                      <Button
                        variant="light"
                        size="sm"
                        onClick={onClose}
                        disabled={submitting}
                      >
                        Cancel
                      </Button>
                      <Button
                        variant="light"
                        color="blue"
                        size="sm"
                        onClick={handleSubmit}
                        loading={submitting}
                        disabled={selectedPipelines.size === 0}
                      >
                        {editMode ? (isMobile ? 'Update' : 'Update Provider') : (isMobile ? 'Add' : 'Add Provider')}
                      </Button>
                    </Group>
                  </Group>
                </Box>
              </Box>
            )}
          </>
        )}
      </Stack>

      {/* Permission Check Modal */}
      <PermissionCheckModal
        opened={permissionModalOpen}
        onClose={() => setPermissionModalOpen(false)}
        metadata={selectedPlugin}
        status={permissionStatus}
        features={permissionFeatures}
        loading={checkingPermissions}
        error={permissionError}
      />
    </StandardModal>
  )
}
