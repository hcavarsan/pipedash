import {
  ActionIcon,
  Box,
  Button,
  Card,
  Divider,
  Group,
  NumberInput,
  SimpleGrid,
  Stack,
  Text,
  Title,
  Tooltip,
} from '@mantine/core'
import { modals } from '@mantine/modals'
import { notifications } from '@mantine/notifications'
import { IconDownload, IconEdit, IconSettings, IconTrash } from '@tabler/icons-react'
import { useCallback, useEffect, useState } from 'react'

import { usePlugins } from '../../../contexts/PluginContext'
import { logger } from '../../../lib/logger'
import { useProviderDetails } from '../../../queries/useProviderDetailsQuery'
import {
  useExportProviders,
  useProviders,
  useRemoveProvider,
  useUpdateProvider,
  useUpdateProviderRefreshInterval,
} from '../../../queries/useProvidersQueries'
import { useProviderStore } from '../../../stores/providerStore'
import type { ProviderConfig } from '../../../types'
import { formatCount } from '../../../utils/formatCount'
import { EmptyState } from '../../common/EmptyState'
import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { AddProviderModal } from '../../provider/AddProviderModal'

export const ProvidersSection = () => {
  const { getPluginDisplayName } = usePlugins()
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)

  const {
    data: providers = [],
    isLoading: loading,
    error: providersError,
    refetch: refetchProviders,
    isRefetching: isRefetchingProviders,
  } = useProviders()

  const { mutateAsync: updateProviderMutation } = useUpdateProvider()
  const { mutateAsync: removeProviderMutation } = useRemoveProvider()
  const updateRefreshIntervalMutation = useUpdateProviderRefreshInterval()
  const exportProvidersMutation = useExportProviders()

  const [editingId, setEditingId] = useState<number | null>(null)
  const [refreshValues, setRefreshValues] = useState<Record<number, number>>({})
  const [editModalOpened, setEditModalOpened] = useState(false)
  const [editingProvider, setEditingProvider] = useState<ProviderConfig | null>(null)
  const [selectedEditId, setSelectedEditId] = useState<number | null>(null)

  const { data: providerDetails, error: providerError } = useProviderDetails(selectedEditId)

  useEffect(() => {
    if (providerDetails && selectedEditId) {
      setEditingProvider(providerDetails)
      setEditModalOpened(true)
      setSelectedEditId(null)
    }
  }, [providerDetails, selectedEditId])

  useEffect(() => {
    if (providerError) {
      notifications.show({
        title: "Couldn't load provider configuration",
        message: 'The provider configuration could not be fetched. Try again.',
        color: 'red',
      })
      setSelectedEditId(null)
    }
  }, [providerError])

  const handleEdit = useCallback((id: number) => {
    setSelectedEditId(id)
  }, [])

  const handleRemove = useCallback(
    (id: number, name: string) => {
      modals.openConfirmModal({
        title: 'Remove provider',
        children: (
          <Text size="sm">
            Are you sure you want to remove <strong>{name}</strong>? All cached pipeline data will be deleted.
          </Text>
        ),
        labels: { confirm: 'Remove provider', cancel: 'Cancel' },
        confirmProps: { color: 'red' },
        onConfirm: async () => {
          try {
            await removeProviderMutation(id)
          } catch (err) {
            logger.error('ProvidersSection', 'Failed to remove provider', err)
          }
        },
      })
    },
    [removeProviderMutation]
  )

  const handleEditRefreshInterval = useCallback((providerId: number, currentInterval: number) => {
    setEditingId(providerId)
    setRefreshValues((prev) => ({
      ...prev,
      [providerId]: currentInterval,
    }))
  }, [])

  const handleSaveRefreshInterval = useCallback(
    (providerId: number, currentInterval: number) => {
      const newValue = refreshValues[providerId] ?? currentInterval

      if (newValue < 5 || newValue > 300) {
        notifications.show({
          title: 'Invalid refresh interval',
          message: 'Refresh interval must be between 5 and 300 seconds',
          color: 'red',
        })

        return
      }

      updateRefreshIntervalMutation.mutate(
        { id: providerId, refreshInterval: newValue },
        {
          onSuccess: () => {
            setEditingId(null)
          },
        }
      )
    },
    [refreshValues, updateRefreshIntervalMutation]
  )

  const handleCancelEdit = useCallback(() => {
    setEditingId(null)
  }, [])

  const handleUpdateProvider = useCallback(
    async (id: number, config: ProviderConfig) => {
      await updateProviderMutation({ id, config })
    },
    [updateProviderMutation]
  )

  const handleCloseEditModal = useCallback(() => {
    setEditModalOpened(false)
    setEditingProvider(null)
  }, [])

  return (
    <>
      <Box>
        <Group justify="space-between" mb="lg">
          <Title order={2} size="lg" fw={600}>
            Providers
          </Title>
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

              return (
                <Card key={provider.id} padding="md">
                  <Stack gap="md">
                    <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="md">
                      <Stack gap={4}>
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
                        </Group>
                      </Stack>

                      <Stack gap={4}>
                        <Text size="xs" c="dimmed">
                          Type
                        </Text>
                        <Text size="sm">{getPluginDisplayName(provider.provider_type)}</Text>
                      </Stack>

                      <Stack gap={4}>
                        <Text size="xs" c="dimmed">
                          Pipelines
                        </Text>
                        <Text size="sm">{formatCount(provider.pipeline_count, 'pipeline')}</Text>
                      </Stack>

                      <Stack gap={4}>
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
                                  [provider.id]: Number(val) || 30,
                                }))
                              }
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
                                variant="subtle"
                                color="gray"
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

      {editingProvider?.id && (
        <AddProviderModal
          opened={editModalOpened}
          onClose={handleCloseEditModal}
          onUpdate={handleUpdateProvider}
          editMode
          existingProvider={editingProvider as ProviderConfig & { id: number }}
        />
      )}
    </>
  )
}
