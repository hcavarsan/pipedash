import { Button, Card, Group, Stack, Switch, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { ErrorState } from '../../../components/common/ErrorState'
import { useDesktopStatus, useUpdateDesktopSettings } from '../queries/useDesktopQueries'
import { useCheckForUpdate, useInstallUpdate, useUpdateStatus } from '../queries/useUpdaterQueries'

export function UpdateCard(): ReactNode {
  const desktop = useDesktopStatus()
  const settings = useUpdateDesktopSettings()
  const { data: status } = useUpdateStatus()
  const check = useCheckForUpdate()
  const install = useInstallUpdate()
  const phase = status?.phase
  const busy = phase === 'downloading' || phase === 'installing'
  const installable = Boolean(status?.version) && (phase === 'available' || phase === 'failed')

  return (
    <Card padding="md">
      <Stack gap="md">
        <Stack gap={0}>
          <Text size="sm" fw={600}>
            Updates
          </Text>
          <Text size="xs" c="dimmed" role="status">
            {status?.message}
          </Text>
        </Stack>
        <Switch
          label="Check for updates automatically"
          description="Pipedash checks for a new release once a day and always asks before installing."
          checked={desktop.data?.settings.auto_check_updates ?? false}
          disabled={settings.busy || !desktop.data}
          onChange={(event) => settings.mutate({ auto_check_updates: event.currentTarget.checked })}
        />
        {settings.error && (
          <ErrorState
            title="Couldn't save the update setting"
            error={settings.error}
            onRetry={settings.retry}
            retrying={settings.busy}
          />
        )}
        <Group gap="xs">
          <Button
            variant="default"
            size="sm"
            loading={phase === 'checking' || check.isPending}
            disabled={busy}
            onClick={() => check.mutate()}
          >
            Check for updates
          </Button>
          {installable && (
            <Button size="sm" loading={install.isPending} onClick={() => install.mutate()}>
              Update and restart
            </Button>
          )}
        </Group>
      </Stack>
    </Card>
  )
}
