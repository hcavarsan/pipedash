import { Box, Button, Card, Divider, Select, Stack, Switch, Text, ThemeIcon } from '@mantine/core'
import { modals } from '@mantine/modals'
import { notifications } from '@mantine/notifications'
import { IconAlertTriangle, IconTrash } from '@tabler/icons-react'
import { useEffect, useState } from 'react'

import { useFactoryReset } from '@/queries/usePlatformQueries'
import { useNotificationStore } from '@/stores/notificationStore'
import { getPlatformOverride, platform, setPlatformOverride } from '@/utils/platform'
import { requestSystemNotificationPermission, systemNotificationsSupported } from '@/utils/systemNotifications'

interface GeneralSectionProps {
  onRefresh?: () => Promise<void>
}

export function GeneralSection({ onRefresh }: GeneralSectionProps) {
  const [platformOverride, setPlatformOverrideState] = useState<string | null>(null)
  const [currentPlatform, setCurrentPlatform] = useState<string>('linux')
  const factoryResetMutation = useFactoryReset()
  const failureNotificationsEnabled = useNotificationStore((s) => s.failureNotificationsEnabled)
  const setFailureNotificationsEnabled = useNotificationStore((s) => s.setFailureNotificationsEnabled)
  const notificationsSupported = systemNotificationsSupported()

  useEffect(() => {
    const loadPlatform = async () => {
      const override = getPlatformOverride()

      setPlatformOverrideState(override)
      const detectedPlatform = await platform()

      setCurrentPlatform(detectedPlatform)
    }

    loadPlatform()
  }, [])

  const handlePlatformChange = (value: string | null) => {
    setPlatformOverride(value as 'macos' | 'windows' | 'linux' | null)
    setPlatformOverrideState(value)
  }

  const handleFailureNotificationsChange = async (enabled: boolean) => {
    if (!enabled) {
      setFailureNotificationsEnabled(false)

      return
    }

    if (!(await requestSystemNotificationPermission())) {
      notifications.show({
        title: 'Notifications blocked',
        message: 'Allow notifications for Pipedash in your browser or system settings, then turn this on again.',
        color: 'orange',
      })

      return
    }

    setFailureNotificationsEnabled(true)
  }

  const handleResetStorage = () => {
    modals.openConfirmModal({
      title: (
        <Box style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <ThemeIcon color="red" variant="light" size="md">
            <IconAlertTriangle size={16} />
          </ThemeIcon>
          <Text fw={600}>Reset Storage</Text>
        </Box>
      ),
      children: (
        <Stack gap="sm">
          <Text size="sm">This will permanently delete all data:</Text>
          <Text size="sm" c="dimmed">
            • All providers and tokens
          </Text>
          <Text size="sm" c="dimmed">
            • All cached data and metrics
          </Text>
          <Text size="sm" c="red" fw={500} mt="xs">
            This action cannot be undone.
          </Text>
        </Stack>
      ),
      labels: { confirm: 'Reset Everything', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: () => {
        factoryResetMutation.mutate(undefined, {
          onSuccess: (result) => {
            localStorage.clear()

            notifications.show({
              title: 'Reset Complete',
              message: `Removed ${result.providers_removed} providers. All settings cleared.`,
              color: 'green',
            })

            setPlatformOverrideState(null)

            if (onRefresh) {
              onRefresh()
            }
          },
        })
      },
    })
  }

  return (
    <Stack gap="md">
      <Card withBorder padding="md" radius="md">
        <Stack gap="md">
          <Box>
            <Text size="md" fw={600}>
              General Settings
            </Text>
            <Text size="xs" c="dimmed">
              Application preferences
            </Text>
          </Box>

          <Divider />

          <Select
            label="Platform"
            description={`Detected: ${currentPlatform}`}
            placeholder="Auto-detect"
            data={[
              { value: '', label: 'Auto-detect' },
              { value: 'macos', label: 'macOS' },
              { value: 'windows', label: 'Windows' },
              { value: 'linux', label: 'Linux' },
            ]}
            value={platformOverride || ''}
            onChange={handlePlatformChange}
          />

          <Switch
            label="Notify when a pipeline fails"
            description={
              notificationsSupported
                ? 'Shows a system notification when a refresh moves a pipeline to failed'
                : 'Browsers allow notifications only over HTTPS or on localhost'
            }
            checked={notificationsSupported && failureNotificationsEnabled}
            onChange={(e) => handleFailureNotificationsChange(e.currentTarget.checked)}
            disabled={!notificationsSupported}
          />
        </Stack>
      </Card>

      <Card withBorder padding="md" radius="md">
        <Stack gap="md">
          <Box>
            <Text size="md" fw={600} c="red">
              Danger Zone
            </Text>
            <Text size="xs" c="dimmed">
              Permanently delete all application data
            </Text>
          </Box>

          <Divider />

          <Button
            leftSection={<IconTrash size={14} />}
            variant="light"
            color="red"
            size="sm"
            fullWidth
            onClick={handleResetStorage}
            loading={factoryResetMutation.isPending}
          >
            Reset Storage
          </Button>
        </Stack>
      </Card>
    </Stack>
  )
}
