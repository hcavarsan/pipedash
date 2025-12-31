import { useCallback, useEffect, useState } from 'react'

import {
  ActionIcon,
  Box,
  Button,
  Card,
  Divider,
  Group,
  Loader,
  NumberInput,
  SimpleGrid,
  Stack,
  Text,
  Tooltip,
} from '@mantine/core'
import { modals } from '@mantine/modals'
import { notifications } from '@mantine/notifications'
import { IconEdit, IconSettings, IconTrash } from '@tabler/icons-react'

import { usePlugins } from '../../../contexts/PluginContext'
import { useProviderDetails } from '../../../queries/useProviderDetailsQuery'
import {
  useProviders,
  useRemoveProvider,
  useUpdateProvider,
  useUpdateProviderRefreshInterval,
} from '../../../queries/useProvidersQueries'
import type { ProviderConfig } from '../../../types'
import { AddProviderModal } from '../../provider/AddProviderModal'

export const ProvidersSection = () => {
  const { getPluginDisplayName } = usePlugins()

  const {
    data: providers = [],
    isLoading: loading,
    error: providersError,
  } = useProviders()

  const { mutateAsync: updateProviderMutation } = useUpdateProvider()
  const { mutateAsync: removeProviderMutation } = useRemoveProvider()
  const updateRefreshIntervalMutation = useUpdateProviderRefreshInterval()

  const error = providersError ? (providersError as Error).message : null

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
        title: 'Error',
        message: 'Failed to load provider configuration',
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
        title: 'Remove Provider',
        children: (
          <Text size="sm">
            Are you sure you want to remove <strong>{name}</strong>? All cached pipeline data will
            be deleted.
          </Text>
        ),
        labels: { confirm: 'Remove', cancel: 'Cancel' },
        confirmProps: { color: 'red' },
        onConfirm: async () => {
          try {
            await removeProviderMutation(id)
          } catch (err) {
            console.error('Failed to remove provider:', err)
          }
        },
      })
    },
    [removeProviderMutation]
  )

  const handleEditRefreshInterval = useCallback(
    (providerId: number, currentInterval: number) => {
      setEditingId(providerId)
      setRefreshValues((prev) => ({
        ...prev,
        [providerId]: currentInterval,
      }))
    },
    []
  )

  const handleSaveRefreshInterval = useCallback(
    (providerId: number, currentInterval: number) => {
      const newValue = refreshValues[providerId] ?? currentInterval

      if (newValue < 5 || newValue > 300) {
        notifications.show({
          title: 'Invalid Value',
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
        <Text size="lg" fw={600} mb="lg">
          Providers
        </Text>

        {loading ? (
          <Stack align="center" py="xl">
            <Loader size="sm" />
          </Stack>
        ) : error ? (
          <Stack align="center" py="xl">
            <Text size="sm" c="red">
              {error}
            </Text>
          </Stack>
        ) : providers.length === 0 ? (
          <Text size="sm" c="dimmed">
            No providers configured. Add a provider from the sidebar.
          </Text>
        ) : (
          <Stack gap="md">
            {providers.map((provider) => {
              const isEditing = editingId === provider.id
              const currentValue = refreshValues[provider.id] ?? provider.refresh_interval

              return (
                <Card key={provider.id} withBorder padding="md" radius="md">
                  <Stack gap="md">
                    <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="md">
                      <Stack gap={4}>
                        <Text size="xs" c="dimmed">
                          Provider Name
                        </Text>
                        <Group gap="xs">
                          <Text size="sm">{provider.name}</Text>
                          {provider.last_fetch_status === 'error' && (
                            <Tooltip
                              label={provider.last_fetch_error || 'Failed to fetch'}
                              multiline
                              w={300}
                              withArrow
                            >
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
                        <Text size="sm">
                          {provider.pipeline_count} pipeline{provider.pipeline_count !== 1 ? 's' : ''}
                        </Text>
                      </Stack>

                      <Stack gap={4}>
                        <Text size="xs" c="dimmed">
                          Refresh Interval
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
                              disabled={updateRefreshIntervalMutation.isPending}
                              size="xs"
                              w={80}
                              suffix="s"
                            />
                          </Group>
                        ) : (
                          <Group
                            gap={2}
                            style={{ cursor: 'pointer' }}
                            onClick={() =>
                              handleEditRefreshInterval(provider.id, provider.refresh_interval)
                            }
                          >
                            <Text size="sm">{provider.refresh_interval}s</Text>
                            <ActionIcon size="xs" variant="transparent" color="gray">
                              <IconEdit size={12} />
                            </ActionIcon>
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
                            onClick={() =>
                              handleSaveRefreshInterval(provider.id, provider.refresh_interval)
                            }
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
                            color="red"
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

      {editingProvider && editingProvider.id && (
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
