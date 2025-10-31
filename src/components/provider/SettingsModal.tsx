import { useState } from 'react'

import {
  Box,
  Button,
  Divider,
  Group,
  NumberInput,
  Stack,
  Text,
} from '@mantine/core'
import { modals } from '@mantine/modals'
import { notifications } from '@mantine/notifications'

import { usePlugins } from '../../contexts/PluginContext'
import { tauriService } from '../../services/tauri'
import type { ProviderSummary } from '../../types'
import { StandardModal } from '../common/StandardModal'

interface SettingsModalProps {
  opened: boolean;
  onClose: () => void;
  providers: ProviderSummary[];
  onRemoveProvider: (id: number, name: string) => Promise<void>;
  onRefresh?: () => Promise<void>;
}

export const SettingsModal = ({
  opened,
  onClose,
  providers,
  onRemoveProvider,
  onRefresh,
}: SettingsModalProps) => {
  const { getPluginDisplayName } = usePlugins()
  const [editingId, setEditingId] = useState<number | null>(null)
  const [refreshValues, setRefreshValues] = useState<Record<number, number>>({})
  const [saving, setSaving] = useState(false)

  const handleRemove = (id: number, name: string) => {
    modals.openConfirmModal({
      title: 'Remove Provider',
      children: (
        <Text size="sm">
          Are you sure you want to remove <strong>{name}</strong>? All cached pipeline data will be deleted.
        </Text>
      ),
      labels: { confirm: 'Remove', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: async () => {
        try {
          await onRemoveProvider(id, name)
        } catch (error) {
          console.error('Failed to remove provider:', error)
        }
      },
    })
  }

  const handleEditRefreshInterval = (provider: ProviderSummary) => {
    setEditingId(provider.id)
    setRefreshValues({
      ...refreshValues,
      [provider.id]: provider.refresh_interval,
    })
  }

  const handleSaveRefreshInterval = async (provider: ProviderSummary) => {
    const newValue = refreshValues[provider.id] ?? provider.refresh_interval

    if (newValue < 5 || newValue > 300) {
      notifications.show({
        title: 'Invalid Value',
        message: 'Refresh interval must be between 5 and 300 seconds',
        color: 'red',
      })
      
return
    }

    setSaving(true)
    try {
      await tauriService.updateProviderRefreshInterval(provider.id, newValue)

      if (onRefresh) {
        await onRefresh()
      }

      notifications.show({
        title: 'Updated',
        message: `Refresh interval updated for ${provider.name}`,
        color: 'green',
      })

      setEditingId(null)
    } catch (error: any) {
      notifications.show({
        title: 'Error',
        message: error?.error || error?.message || 'Failed to update',
        color: 'red',
      })
    } finally {
      setSaving(false)
    }
  }

  const handleCancelEdit = () => {
    setEditingId(null)
  }

  return (
    <StandardModal
      opened={opened}
      onClose={onClose}
      title="Settings"
    >
      <Stack gap="xl">
        <Box>
          <Text size="sm" fw={600} c="dimmed" mb="md" tt="uppercase">
            Providers
          </Text>

          {providers.length === 0 ? (
            <Text size="sm" c="dimmed" ta="center" py="xl">
              No providers configured
            </Text>
          ) : (
            <Stack gap="md">
              {providers.map((provider) => {
                const isEditing = editingId === provider.id
                const currentValue = refreshValues[provider.id] ?? provider.refresh_interval

                return (
                  <Box
                    key={provider.id}
                    p="lg"
                    style={{
                      border: '1px solid var(--mantine-color-dark-5)',
                      borderRadius: '8px',
                      backgroundColor: 'var(--mantine-color-dark-8)',
                    }}
                  >
                    <Stack gap="md">
                      <Group justify="space-between" align="flex-start" wrap="wrap">
                        <Box style={{ flex: 1 }}>
                          <Text fw={600} size="md" mb={4}>
                            {provider.name}
                          </Text>
                          <Text size="sm" c="dimmed">
                            {getPluginDisplayName(provider.provider_type)} · {provider.pipeline_count} pipeline{provider.pipeline_count !== 1 ? 's' : ''}
                          </Text>
                        </Box>

                        {!isEditing && (
                          <Button
                            size="xs"
                            color="red"
                            variant="subtle"
                            onClick={() => handleRemove(provider.id, provider.name)}
                          >
                            Remove
                          </Button>
                        )}
                      </Group>

                      <Divider />

                      <Group align="flex-end" gap="md" wrap="wrap">
                        <NumberInput
                          label="Refresh Interval"
                          description="Seconds between data fetches (5-300)"
                          value={currentValue}
                          onChange={(val) =>
                            setRefreshValues({
                              ...refreshValues,
                              [provider.id]: Number(val) || 30,
                            })
                          }
                          min={5}
                          max={300}
                          step={5}
                          disabled={!isEditing || saving}
                          style={{ flex: 1, maxWidth: 200 }}
                        />

                        {isEditing ? (
                          <Group gap="xs">
                            <Button
                              size="xs"
                              variant="subtle"
                              color="gray"
                              onClick={handleCancelEdit}
                              disabled={saving}
                            >
                              Cancel
                            </Button>
                            <Button
                              size="xs"
                              onClick={() => handleSaveRefreshInterval(provider)}
                              loading={saving}
                            >
                              Save
                            </Button>
                          </Group>
                        ) : (
                          <Button
                            size="xs"
                            variant="light"
                            onClick={() => handleEditRefreshInterval(provider)}
                          >
                            Edit
                          </Button>
                        )}
                      </Group>
                    </Stack>
                  </Box>
                )
              })}
            </Stack>
          )}
        </Box>

        <Divider />

        <Box>
          <Text size="sm" fw={600} c="dimmed" mb="sm" tt="uppercase">
            Application
          </Text>
          <Group justify="space-between">
            <Text size="sm" c="dimmed">Version</Text>
            <Text size="sm" fw={500}>0.1.0</Text>
          </Group>
        </Box>
      </Stack>
    </StandardModal>
  )
}
