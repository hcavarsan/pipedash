import {
  ActionIcon,
  Box,
  Button,
  Card,
  Divider,
  FileButton,
  Group,
  Loader,
  NumberInput,
  SimpleGrid,
  Stack,
  Text,
  Tooltip,
} from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconDownload, IconEdit, IconExternalLink, IconSettings, IconTrash, IconUpload } from '@tabler/icons-react'
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react'

import { usePlugins } from '../../../contexts/PluginContext'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useNow } from '../../../hooks/useNow'
import {
  useExportProviders,
  useImportProviders,
  useProviders,
  useUpdateProviderRefreshInterval,
} from '../../../queries/useProvidersQueries'
import { useStorageConfig } from '../../../queries/useStorageQueries'
import { useProviderStore } from '../../../stores/providerStore'
import { HEALTH_META } from '../../../theme/status'
import type { ProviderSummary } from '../../../types'
import { errorMessage } from '../../../utils/errorDisplay'
import { formatCount } from '../../../utils/formatCount'
import { formatDateTime, formatTime } from '../../../utils/formatDateTime'
import { formatRelativeTime } from '../../../utils/formatRelativeTime'
import { openExternal } from '../../../utils/openExternal'
import { EmptyState } from '../../common/EmptyState'
import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { IosGroup, IosRow } from '../../ios/IosList'
import { useConfirmRemoveProvider } from '../../provider/ProviderModals'
import { AccentText, FieldRow, RowNumberInput } from '../mobileParts'

const CONNECTION_NOTE = 'Pipedash reports connection problems.'

const getStatusDetails = (provider: ProviderSummary, statusPagesEnabled: boolean, now: number) => {
  const { status_page: page, signals, level } = provider.health
  const upcoming = statusPagesEnabled
    ? signals
        .filter((signal) => signal.evidence === 'fresh')
        .flatMap((signal) => signal.maintenances)
        .flatMap((item) =>
          !item.in_progress && item.starts_at ? [{ id: item.id, name: item.name, at: item.starts_at }] : []
        )
    : []
  const reportedBy = signals.find((signal) => signal.kind === 'status_page')?.reported_by ?? page?.name
  const summary = page
    ? [
        reportedBy,
        HEALTH_META[level].label,
        !page.error && page.checked_at && `checked ${formatRelativeTime(page.checked_at, now)}`,
      ]
        .filter(Boolean)
        .join(' · ')
    : null

  return { page, upcoming, summary }
}

const ProviderStatusInfo = ({
  provider,
  statusPagesEnabled,
  now,
}: {
  provider: ProviderSummary
  statusPagesEnabled: boolean
  now: number
}) => {
  const { page, upcoming, summary } = getStatusDetails(provider, statusPagesEnabled, now)

  return (
    <Stack gap="3xs">
      <Text size="xs" c="dimmed">
        Status
      </Text>
      {statusPagesEnabled && page ? (
        <Group gap="xs" wrap="wrap">
          <Text size="sm">{summary}</Text>
          <Button
            size="compact-xs"
            variant="subtle"
            color="gray"
            rightSection={<IconExternalLink size={14} aria-hidden />}
            aria-label={`Open ${page.name} status page`}
            onClick={() => {
              void openExternal(page.url)
            }}
          >
            Open status page
          </Button>
        </Group>
      ) : (
        <Text size="sm" c="dimmed">
          {statusPagesEnabled ? `No public status page. ${CONNECTION_NOTE}` : CONNECTION_NOTE}
        </Text>
      )}
      {statusPagesEnabled && page?.error && (
        <Text size="xs" c="orange">
          {page.error}
          {page.checked_at && ` · last successful check ${formatDateTime(page.checked_at)}`}
        </Text>
      )}
      {upcoming.map((item) => (
        <Text key={item.id} size="xs" c="dimmed">
          Maintenance: {item.name} · {formatDateTime(item.at)}
        </Text>
      ))}
    </Stack>
  )
}

const ProviderStatusRows = ({
  provider,
  statusPagesEnabled,
  now,
}: {
  provider: ProviderSummary
  statusPagesEnabled: boolean
  now: number
}) => {
  const { page, upcoming, summary } = getStatusDetails(provider, statusPagesEnabled, now)
  const note = statusPagesEnabled ? `No public status page. ${CONNECTION_NOTE}` : CONNECTION_NOTE

  return (
    <>
      <IosRow title="Status" subtitle={statusPagesEnabled && page ? summary : note} />
      {statusPagesEnabled && page?.error && (
        <IosRow
          title="Status page check failed"
          subtitle={
            <Text span inherit c="orange">
              {page.error}
              {page.checked_at && ` · last successful check ${formatDateTime(page.checked_at)}`}
            </Text>
          }
        />
      )}
      {upcoming.map((item) => (
        <IosRow key={item.id} title="Maintenance" subtitle={`${item.name} · ${formatDateTime(item.at)}`} />
      ))}
      {statusPagesEnabled && page && (
        <IosRow
          title="Open status page"
          ariaLabel={`Open ${page.name} status page`}
          accessory={<IconExternalLink size={18} aria-hidden />}
          onClick={() => {
            void openExternal(page.url)
          }}
        />
      )}
    </>
  )
}

interface ProvidersSectionProps {
  onHeaderActionsChange?: ((actions: ReactNode) => void) | undefined
}

export const ProvidersSection = ({ onHeaderActionsChange }: ProvidersSectionProps): ReactNode => {
  const mobileShell = useMobileShell()
  const { getPluginDisplayName } = usePlugins()
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const handleEdit = useProviderStore((s) => s.requestEditProvider)
  const handleRemove = useConfirmRemoveProvider()

  const {
    data: providers = [],
    isLoading: loading,
    error: providersError,
    refetch: refetchProviders,
    isRefetching: isRefetchingProviders,
  } = useProviders()

  const updateRefreshIntervalMutation = useUpdateProviderRefreshInterval(mobileShell)
  const exportProvidersMutation = useExportProviders()
  const importProvidersMutation = useImportProviders()
  const importResetRef = useRef<() => void>(null)
  const now = useNow(30_000)
  const statusPagesEnabled = useStorageConfig().data?.config.general.status_pages_enabled !== false

  const [editingId, setEditingId] = useState<number | null>(null)
  const [refreshValues, setRefreshValues] = useState<Record<number, number | string>>({})
  const [refreshTouched, setRefreshTouched] = useState(false)

  const handleImport = useCallback(
    (file: File | null) => {
      if (file) {
        importProvidersMutation.mutate(file)
        importResetRef.current?.()
      }
    },
    [importProvidersMutation.mutate]
  )

  useEffect(() => {
    if (mobileShell || !onHeaderActionsChange) {
      return
    }

    onHeaderActionsChange(
      <Group gap="xs">
        <FileButton accept=".toml" resetRef={importResetRef} onChange={handleImport}>
          {(props) => (
            <Button
              {...props}
              size="compact-sm"
              variant="light"
              color="gray"
              leftSection={<IconUpload size={14} />}
              loading={importProvidersMutation.isPending}
              disabled={loading}
            >
              Import
            </Button>
          )}
        </FileButton>
        <Button
          size="compact-sm"
          variant="light"
          color="gray"
          leftSection={<IconDownload size={14} />}
          onClick={() => exportProvidersMutation.mutate()}
          loading={exportProvidersMutation.isPending}
          disabled={loading || providers.length === 0}
        >
          Export
        </Button>
      </Group>
    )

    return () => onHeaderActionsChange(null)
  }, [
    mobileShell,
    onHeaderActionsChange,
    handleImport,
    loading,
    providers.length,
    importProvidersMutation.isPending,
    exportProvidersMutation.isPending,
    exportProvidersMutation.mutate,
  ])

  const handleEditRefreshInterval = useCallback(
    (providerId: number, currentInterval: number) => {
      setEditingId(providerId)
      setRefreshTouched(false)
      updateRefreshIntervalMutation.reset()
      setRefreshValues((prev) => ({
        ...prev,
        [providerId]: currentInterval,
      }))
    },
    [updateRefreshIntervalMutation.reset]
  )

  const handleSaveRefreshInterval = useCallback(
    (providerId: number, currentInterval: number) => {
      const newValue = refreshValues[providerId] ?? currentInterval

      if (newValue === '' || !Number.isInteger(Number(newValue)) || Number(newValue) < 5 || Number(newValue) > 300) {
        setRefreshTouched(true)
        if (mobileShell) {
          notifications.show({
            title: 'Invalid refresh interval',
            message: 'Refresh interval must be between 5 and 300 seconds',
            color: 'red',
          })
        }

        return
      }

      updateRefreshIntervalMutation.mutate(
        { id: providerId, refreshInterval: Number(newValue) },
        {
          onSuccess: () => {
            setEditingId(null)
          },
        }
      )
    },
    [refreshValues, updateRefreshIntervalMutation, mobileShell]
  )

  const handleCancelEdit = useCallback(() => {
    setEditingId(null)
  }, [])

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        {loading ? (
          <LoadingState message="Loading providers..." />
        ) : providersError ? (
          <ErrorState
            title="Couldn't load providers"
            error={providersError}
            onRetry={() => refetchProviders()}
            retrying={isRefetchingProviders}
          />
        ) : providers.length === 0 ? (
          <EmptyState
            title="No providers configured"
            description="Add a CI/CD provider to start monitoring pipelines."
            action={
              <Button size="sm" onClick={openAddProviderModal}>
                Add provider
              </Button>
            }
          />
        ) : (
          providers.map((provider) => {
            const isEditing = editingId === provider.id
            const currentValue = refreshValues[provider.id] ?? provider.refresh_interval
            const saving = updateRefreshIntervalMutation.isPending

            return (
              <Stack key={provider.id} gap="sm">
                <IosGroup>
                  <IosRow
                    title={provider.name}
                    subtitle={[
                      getPluginDisplayName(provider.provider_type),
                      formatCount(provider.pipeline_count, 'pipeline'),
                    ].join(' · ')}
                  />
                  {provider.last_fetch_status === 'error' && (
                    <IosRow
                      title={
                        <Text span inherit c="red">
                          Fetch failed
                        </Text>
                      }
                      subtitle={provider.last_fetch_error || 'Failed to fetch'}
                    />
                  )}
                  {provider.paused_until && (
                    <IosRow
                      title={`Paused until ${formatTime(provider.paused_until)}`}
                      subtitle="The API quota is almost used up. Pipedash resumes refreshing at the reset time."
                    />
                  )}
                  {isEditing ? (
                    <>
                      <FieldRow title="Refresh interval">
                        {(id) => (
                          <RowNumberInput
                            id={id}
                            value={currentValue}
                            onChange={(val) =>
                              setRefreshValues((prev) => ({ ...prev, [provider.id]: Number(val) || 30 }))
                            }
                            min={5}
                            max={300}
                            step={5}
                            suffix="s"
                            aria-label={`Refresh interval in seconds for ${provider.name}`}
                            disabled={saving}
                          />
                        )}
                      </FieldRow>
                      <IosRow
                        title={<AccentText>Save</AccentText>}
                        disabled={saving}
                        onClick={() => handleSaveRefreshInterval(provider.id, provider.refresh_interval)}
                        accessory={saving ? <Loader size={18} /> : undefined}
                      />
                      <IosRow title="Cancel" disabled={saving} onClick={handleCancelEdit} />
                    </>
                  ) : (
                    <IosRow
                      title="Refresh interval"
                      value={`${provider.refresh_interval}s`}
                      chevron
                      ariaLabel={`Edit refresh interval for ${provider.name}`}
                      onClick={() => handleEditRefreshInterval(provider.id, provider.refresh_interval)}
                    />
                  )}
                  <ProviderStatusRows provider={provider} statusPagesEnabled={statusPagesEnabled} now={now} />
                  <IosRow title="Edit provider" chevron onClick={() => handleEdit(provider.id)} />
                </IosGroup>
                <IosGroup>
                  <IosRow
                    title="Remove provider"
                    destructive
                    ariaLabel={`Remove ${provider.name}`}
                    onClick={() => handleRemove(provider.id, provider.name)}
                  />
                </IosGroup>
              </Stack>
            )
          })
        )}

        <IosGroup>
          <FileButton
            accept=".toml"
            resetRef={importResetRef}
            onChange={(file) => {
              if (file) {
                importProvidersMutation.mutate(file)
                importResetRef.current?.()
              }
            }}
          >
            {(props) => (
              <IosRow
                title="Import providers"
                disabled={loading || importProvidersMutation.isPending}
                onClick={props.onClick}
                accessory={importProvidersMutation.isPending ? <Loader size={18} /> : <IconUpload size={18} />}
              />
            )}
          </FileButton>
          <IosRow
            title="Export providers"
            disabled={loading || providers.length === 0 || exportProvidersMutation.isPending}
            onClick={() => exportProvidersMutation.mutate()}
            accessory={exportProvidersMutation.isPending ? <Loader size={18} /> : <IconDownload size={18} />}
          />
        </IosGroup>
      </Stack>
    )
  }

  return (
    <Box>
      {loading ? (
        <LoadingState message="Loading providers..." />
      ) : providersError ? (
        <ErrorState
          title="Couldn't load providers"
          error={providersError}
          onRetry={() => refetchProviders()}
          retrying={isRefetchingProviders}
        />
      ) : providers.length === 0 ? (
        <EmptyState
          title="No providers configured"
          description="Add a CI/CD provider to start monitoring pipelines."
          action={
            <Button size="sm" onClick={openAddProviderModal}>
              Add provider
            </Button>
          }
        />
      ) : (
        <Stack gap="md">
          {providers.map((provider) => {
            const isEditing = editingId === provider.id
            const currentValue = refreshValues[provider.id] ?? provider.refresh_interval
            const invalidInterval =
              currentValue === '' ||
              !Number.isInteger(Number(currentValue)) ||
              Number(currentValue) < 5 ||
              Number(currentValue) > 300
            const intervalError =
              refreshTouched && invalidInterval
                ? 'Enter a whole number from 5 to 300 seconds.'
                : updateRefreshIntervalMutation.isError && updateRefreshIntervalMutation.variables?.id === provider.id
                  ? errorMessage(updateRefreshIntervalMutation.error, 'Try saving the interval again.')
                  : undefined

            return (
              <Card key={provider.id} padding="md">
                <Stack gap="md">
                  <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="md">
                    <Stack gap="3xs">
                      <Text size="xs" c="dimmed">
                        Provider name
                      </Text>
                      <Group gap="xs">
                        <Text size="sm">{provider.name}</Text>
                        {provider.last_fetch_status === 'error' && (
                          <Tooltip label={provider.last_fetch_error || 'Failed to fetch'} multiline w={300}>
                            <Text size="xs" c="red">
                              Error
                            </Text>
                          </Tooltip>
                        )}
                        {provider.paused_until && (
                          <Tooltip
                            label="The API quota is almost used up. Pipedash resumes refreshing at the reset time."
                            multiline
                            w={300}
                          >
                            <Text size="xs" c="dimmed">
                              Paused until {formatTime(provider.paused_until)}
                            </Text>
                          </Tooltip>
                        )}
                      </Group>
                    </Stack>

                    <Stack gap="3xs">
                      <Text size="xs" c="dimmed">
                        Type
                      </Text>
                      <Text size="sm">{getPluginDisplayName(provider.provider_type)}</Text>
                    </Stack>

                    <Stack gap="3xs">
                      <Text size="xs" c="dimmed">
                        Pipelines
                      </Text>
                      <Text size="sm">{formatCount(provider.pipeline_count, 'pipeline')}</Text>
                    </Stack>

                    <Stack gap="3xs">
                      <Text size="xs" c="dimmed">
                        Refresh interval
                      </Text>
                      {isEditing ? (
                        <Group gap="xs">
                          <NumberInput
                            value={currentValue}
                            onChange={(val) =>
                              setRefreshValues((prev) => ({
                                ...prev,
                                [provider.id]: val,
                              }))
                            }
                            onBlur={() => setRefreshTouched(true)}
                            error={intervalError}
                            clampBehavior="none"
                            min={5}
                            max={300}
                            step={5}
                            aria-label={`Refresh interval in seconds for ${provider.name}`}
                            disabled={updateRefreshIntervalMutation.isPending}
                            size="xs"
                            w={80}
                            suffix="s"
                          />
                        </Group>
                      ) : (
                        <Group gap="xs">
                          <Text size="sm">{provider.refresh_interval}s</Text>
                          <Tooltip label={`Edit refresh interval for ${provider.name}`}>
                            <ActionIcon
                              size="sm"
                              aria-label={`Edit refresh interval for ${provider.name}`}
                              onClick={() => handleEditRefreshInterval(provider.id, provider.refresh_interval)}
                            >
                              <IconEdit size={14} />
                            </ActionIcon>
                          </Tooltip>
                        </Group>
                      )}
                    </Stack>
                  </SimpleGrid>

                  <ProviderStatusInfo provider={provider} statusPagesEnabled={statusPagesEnabled} now={now} />

                  <Divider />

                  <Group gap="xs" justify="flex-end">
                    {isEditing ? (
                      <>
                        <Button
                          size="compact-xs"
                          variant="subtle"
                          color="gray"
                          onClick={handleCancelEdit}
                          disabled={updateRefreshIntervalMutation.isPending}
                        >
                          Cancel
                        </Button>
                        <Button
                          size="compact-xs"
                          variant="light"
                          color="gray"
                          onClick={() => handleSaveRefreshInterval(provider.id, provider.refresh_interval)}
                          loading={updateRefreshIntervalMutation.isPending}
                        >
                          Save
                        </Button>
                      </>
                    ) : (
                      <>
                        <Button
                          size="compact-xs"
                          variant="subtle"
                          color="gray"
                          onClick={() => handleEdit(provider.id)}
                          leftSection={<IconSettings size={14} />}
                        >
                          Edit
                        </Button>
                        <Button
                          size="compact-xs"
                          variant="subtle"
                          color="gray"
                          onClick={() => handleRemove(provider.id, provider.name)}
                          leftSection={<IconTrash size={14} />}
                        >
                          Remove
                        </Button>
                      </>
                    )}
                  </Group>
                </Stack>
              </Card>
            )
          })}
        </Stack>
      )}
    </Box>
  )
}
