import {
  Button,
  Card,
  Divider,
  Input,
  type MantineColorScheme,
  SegmentedControl,
  Select,
  Stack,
  Switch,
  Text,
  Title,
  useMantineColorScheme,
} from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconTrash } from '@tabler/icons-react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'

import { logger } from '@/lib/logger'
import { queryKeys } from '@/lib/queryKeys'
import { useStorageConfig } from '@/queries/useStorageQueries'
import { service } from '@/services'
import { useNotificationStore } from '@/stores/notificationStore'
import { displayErrorNotification } from '@/utils/errorDisplay'
import { getPlatformOverride, platform, setPlatformOverride } from '@/utils/platform'
import { requestSystemNotificationPermission, systemNotificationsSupported } from '@/utils/systemNotifications'

import { useFactoryResetConfirm } from '../useFactoryResetConfirm'

interface GeneralSectionProps {
  onRefresh?: () => Promise<void>
}

const THEME_OPTIONS = [
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
  { value: 'auto', label: 'System' },
]

export function GeneralSection({ onRefresh }: GeneralSectionProps) {
  const [platformOverride, setPlatformOverrideState] = useState<string | null>(null)
  const [currentPlatform, setCurrentPlatform] = useState<string>('linux')
  const { colorScheme, setColorScheme } = useMantineColorScheme()
  const { confirmReset, isResetting } = useFactoryResetConfirm(async () => {
    setPlatformOverrideState(null)
    await onRefresh?.()
  })
  const failureNotificationsEnabled = useNotificationStore((s) => s.failureNotificationsEnabled)
  const setFailureNotificationsEnabled = useNotificationStore((s) => s.setFailureNotificationsEnabled)
  const providerAlertsEnabled = useNotificationStore((s) => s.providerAlertsEnabled)
  const setProviderAlertsEnabled = useNotificationStore((s) => s.setProviderAlertsEnabled)
  const notificationsSupported = systemNotificationsSupported()

  const queryClient = useQueryClient()
  const general = useStorageConfig().data?.config.general
  const statusPagesMutation = useMutation({
    mutationFn: (enabled: boolean) => {
      if (!general) {
        throw new Error('Settings are not loaded yet')
      }

      return service.updateGeneralSettings({
        metrics_enabled: general.metrics_enabled,
        default_refresh_interval: general.default_refresh_interval,
        status_pages_enabled: enabled,
      })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.storage.config() }),
    onError: (error) => {
      logger.error('GeneralSection', 'Failed to update status page setting', error)
      displayErrorNotification(error, "Couldn't update status page setting")
    },
  })
  const statusPagesChecked = statusPagesMutation.isPending
    ? statusPagesMutation.variables
    : (general?.status_pages_enabled ?? false)

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

  const handleNotificationToggle = async (enabled: boolean, setEnabled: (enabled: boolean) => void) => {
    if (!enabled) {
      setEnabled(false)

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

    setEnabled(true)
  }

  return (
    <>
      <Title order={2} size="lg" fw={600} mb="lg">
        General
      </Title>

      <Stack gap="md">
        <Card padding="md">
          <Stack gap="md">
            <Input.Wrapper label="Theme" description="Choose Dark, Light, or follow your system setting.">
              <SegmentedControl
                mt="xs"
                fullWidth
                data={THEME_OPTIONS}
                value={colorScheme}
                onChange={(value) => setColorScheme(value as MantineColorScheme)}
              />
            </Input.Wrapper>

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

            <Divider />

            <Switch
              label="Notify when a pipeline fails"
              description={
                notificationsSupported
                  ? 'Show a system notification when a refresh finds a pipeline that just failed.'
                  : 'Browsers allow notifications only over HTTPS or on localhost.'
              }
              checked={notificationsSupported && failureNotificationsEnabled}
              onChange={(e) => handleNotificationToggle(e.currentTarget.checked, setFailureNotificationsEnabled)}
              disabled={!notificationsSupported}
            />

            <Divider />

            <Switch
              label="Check provider status pages"
              description="Reads the public status page of each provider, or the one you configured. No credentials are sent."
              checked={statusPagesChecked}
              onChange={(e) => statusPagesMutation.mutate(e.currentTarget.checked)}
              disabled={!general}
            />

            <Divider />

            <Switch
              label="Notify me about provider outages"
              description={
                notificationsSupported
                  ? 'Show a system notification when a provider reports an outage or recovers.'
                  : 'Browsers allow notifications only over HTTPS or on localhost.'
              }
              checked={notificationsSupported && providerAlertsEnabled}
              onChange={(e) => handleNotificationToggle(e.currentTarget.checked, setProviderAlertsEnabled)}
              disabled={!notificationsSupported}
            />
          </Stack>
        </Card>

        <Card padding="md">
          <Stack gap="md">
            <Stack gap={0}>
              <Text size="sm" fw={600} c="red">
                Danger zone
              </Text>
              <Text size="xs" c="dimmed">
                Permanently delete all application data
              </Text>
            </Stack>

            <Divider />

            <Button
              leftSection={<IconTrash size={16} />}
              variant="light"
              color="red"
              fullWidth
              onClick={confirmReset}
              loading={isResetting}
            >
              Factory reset
            </Button>
          </Stack>
        </Card>
      </Stack>
    </>
  )
}
