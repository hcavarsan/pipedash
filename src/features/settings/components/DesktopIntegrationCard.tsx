import { Button, Card, Group, Stack, Switch, Text } from '@mantine/core'
import { platform } from '@tauri-apps/plugin-os'
import type { ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'

import { ErrorState } from '../../../components/common/ErrorState'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { usePipelineViewStore } from '../../pipelines/stores/pipelineViewStore'
import { useDesktopStatus, useUpdateDesktopSettings } from '../queries/useDesktopQueries'

const TRAY_STATUS_TEXT = {
  offline: 'offline',
  failing: 'a pinned pipeline is failing',
  running: 'a watched run or pinned pipeline is running',
  attention: '',
  idle: 'nothing to report',
} as const

export function DesktopIntegrationCard(): ReactNode {
  const os = platform()
  const navigate = useNavigate()
  const status = useDesktopStatus()
  const settings = useUpdateDesktopSettings()
  const current = status.data
  const error = settings.error ?? status.error ?? current?.error
  const busy = settings.busy
  let state = 'Desktop menu is off'
  if (current?.tray_state === 'error') {
    state = 'Desktop menu needs attention'
  } else if (current?.tray_state === 'ready') {
    state = os === 'macos' ? 'Menu bar item created' : 'System tray icon created'
  }
  let liveMessage = 'Loading desktop settings'
  if (busy) {
    liveMessage = 'Saving desktop settings'
  } else if (current) {
    liveMessage = state
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Text size="sm" fw={600}>
          Desktop integration
        </Text>
        <LiveStatus message={liveMessage} />
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
              ? 'Closing minimizes the window, so Pipedash stays reachable even if your desktop has no tray.'
              : 'Closing hides the window. Use Quit Pipedash to stop monitoring.'
          }
          checked={current?.settings.keep_running_on_close ?? false}
          disabled={busy || !current?.settings.show_tray || current.tray_state !== 'ready'}
          onChange={(event) => settings.mutate({ keep_running_on_close: event.currentTarget.checked })}
        />
        {current?.settings.show_tray && (
          <Text size="xs" c="dimmed">
            {`Menu bar status: ${[current.unread > 0 && `${current.unread} unread`, TRAY_STATUS_TEXT[current.tray_status]].filter(Boolean).join(', ')}`}
          </Text>
        )}
        <Text size="xs" c="dimmed">
          {`The icon shows a status dot: red when a pinned pipeline fails, blue while a watched run or pinned pipeline runs, amber for unread notifications.${os === 'macos' ? ' The menu bar also shows the unread count.' : ''}`}
        </Text>
        {error && (
          <ErrorState
            title="Desktop integration needs attention"
            error={error}
            onRetry={settings.retry}
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
