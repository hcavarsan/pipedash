import { Button, Card, Group, Stack, Switch, Text } from '@mantine/core'
import { platform } from '@tauri-apps/plugin-os'
import type { ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'

import { ErrorState } from '../../../components/common/ErrorState'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { usePipelineViewStore } from '../../pipelines/stores/pipelineViewStore'
import { useDesktopStatus, useUpdateDesktopSettings } from '../queries/useDesktopQueries'

export function DesktopIntegrationCard(): ReactNode {
  const os = platform()
  const navigate = useNavigate()
  const status = useDesktopStatus()
  const settings = useUpdateDesktopSettings()
  const current = status.data
  const error = settings.error ?? status.error ?? current?.error
  const busy = settings.busy
  const state =
    current?.tray_state === 'error'
      ? 'Desktop menu needs attention'
      : current?.tray_state === 'ready'
        ? os === 'macos'
          ? 'Menu bar item created'
          : 'System tray icon created'
        : 'Desktop menu is off'

  return (
    <Card padding="md">
      <Stack gap="md">
        <Text size="sm" fw={600}>
          Desktop integration
        </Text>
        <LiveStatus message={busy ? 'Saving desktop settings' : current ? state : 'Loading desktop settings'} />
        <Switch
          label={os === 'macos' ? 'Show in menu bar' : 'Show in system tray'}
          checked={current?.settings.show_tray ?? false}
          disabled={busy || !current}
          onChange={(event) => settings.mutate({ show_tray: event.currentTarget.checked })}
        />
        <Switch
          label="Keep running when the window closes"
          aria-label="Keep running when the window closes"
          description={
            os === 'linux'
              ? 'Closing minimizes the window so it stays accessible even without a tray host.'
              : 'Closing hides the window. Use Quit Pipedash to stop monitoring.'
          }
          checked={current?.settings.keep_running_on_close ?? false}
          disabled={busy || !current?.settings.show_tray || current.tray_state !== 'ready'}
          onChange={(event) => settings.mutate({ keep_running_on_close: event.currentTarget.checked })}
        />
        {os === 'linux' && (
          <Text size="xs" c="dimmed">
            Your desktop may need a tray extension. Closing minimizes Pipedash so you can still reopen it.
          </Text>
        )}
        {error && (
          <ErrorState
            title="Desktop integration needs attention"
            error={error}
            onRetry={() => settings.mutate({})}
            retrying={busy}
          />
        )}
        <Group gap="xs">
          <Button
            variant="default"
            size="sm"
            onClick={async () => {
              await navigate('/pipelines?view=pinned')
              usePipelineViewStore.getState().openEditor('pinned')
            }}
          >
            Manage pinned pipelines
          </Button>
          <Button variant="subtle" size="sm" onClick={() => void navigate('/settings/notifications')}>
            Notification settings
          </Button>
        </Group>
      </Stack>
    </Card>
  )
}
