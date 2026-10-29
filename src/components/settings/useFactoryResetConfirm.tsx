import { Group, Stack, Text, ThemeIcon, useMantineColorScheme } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'

import { useMobileShell } from '@/hooks/useIsMobile'
import { useFactoryReset } from '@/queries/usePlatformQueries'
import { openConfirm } from '@/utils/confirm'

interface FactoryResetConfirmResult {
  confirmReset: () => void
  isResetting: boolean
}

export const useFactoryResetConfirm = (onReset?: () => Promise<void> | void): FactoryResetConfirmResult => {
  const { mutate, isPending } = useFactoryReset()
  const { clearColorScheme } = useMantineColorScheme()
  const mobileShell = useMobileShell()

  const confirmReset = () => {
    openConfirm({
      title: mobileShell ? (
        'Factory reset'
      ) : (
        <Group gap="xs">
          <ThemeIcon color="red" variant="transparent" size="md">
            <IconAlertTriangle size={16} aria-hidden />
          </ThemeIcon>
          <Text fw={600}>Factory reset</Text>
        </Group>
      ),
      children: mobileShell ? (
        <Text size="sm" ta="center">
          This deletes providers and their saved tokens, cached data, metrics and app preferences. Storage
          configuration, external secrets, backups and exports are kept. This action cannot be undone.
        </Text>
      ) : (
        <Stack gap="xs">
          <Text size="sm">This permanently deletes:</Text>
          <Text size="sm" c="dimmed">
            • Providers and their saved tokens
          </Text>
          <Text size="sm" c="dimmed">
            • All cached data and metrics
          </Text>
          <Text size="sm" c="dimmed">
            • Theme and platform preferences
          </Text>
          <Text size="sm" c="dimmed">
            Storage configuration, external secrets, backups and exports are kept.
          </Text>
          <Text size="sm" c="red" fw={500}>
            This action cannot be undone.
          </Text>
        </Stack>
      ),
      labels: { confirm: 'Reset everything', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: () => {
        mutate(undefined, {
          onSuccess: async () => {
            clearColorScheme()
            await onReset?.()
          },
        })
      },
    })
  }

  return { confirmReset, isResetting: isPending }
}
