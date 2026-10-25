import { Group, Stack, Text, ThemeIcon, useMantineColorScheme } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'

import { useMobileShell } from '@/hooks/useIsMobile'
import { useFactoryReset } from '@/queries/usePlatformQueries'
import { openConfirm } from '@/utils/confirm'

export const useFactoryResetConfirm = (onReset?: () => Promise<void> | void) => {
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
            <IconAlertTriangle size={16} />
          </ThemeIcon>
          <Text fw={600}>Factory reset</Text>
        </Group>
      ),
      children: mobileShell ? (
        <Text size="sm" ta="center">
          This permanently deletes all providers and tokens, cached data and metrics, and your theme and platform
          preferences. This action cannot be undone.
        </Text>
      ) : (
        <Stack gap="xs">
          <Text size="sm">This permanently deletes all data and local settings:</Text>
          <Text size="sm" c="dimmed">
            • All providers and tokens
          </Text>
          <Text size="sm" c="dimmed">
            • All cached data and metrics
          </Text>
          <Text size="sm" c="dimmed">
            • Theme and platform preferences
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
