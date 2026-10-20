import { Group, Stack, Text, ThemeIcon, useMantineColorScheme } from '@mantine/core'
import { modals } from '@mantine/modals'
import { IconAlertTriangle } from '@tabler/icons-react'

import { useFactoryReset } from '@/queries/usePlatformQueries'

export const useFactoryResetConfirm = (onReset?: () => Promise<void> | void) => {
  const { mutate, isPending } = useFactoryReset()
  const { clearColorScheme } = useMantineColorScheme()

  const confirmReset = () => {
    modals.openConfirmModal({
      title: (
        <Group gap="xs">
          <ThemeIcon color="red" variant="transparent" size="md">
            <IconAlertTriangle size={16} />
          </ThemeIcon>
          <Text fw={600}>Factory reset</Text>
        </Group>
      ),
      children: (
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
