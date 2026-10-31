import { Button, Card, Divider, Group, Progress, Stack, Switch, Text } from '@mantine/core'
import { useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useRef } from 'react'

import { LoadingState } from '../../../components/common/LoadingState'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { queryKeys } from '../../../lib/queryKeys'
import type { UpdateStatus } from '../../../types/updater'
import { openConfirm } from '../../../utils/confirm'
import { displayErrorNotification, errorMessage } from '../../../utils/errorDisplay'
import { formatBytes } from '../../../utils/formatBytes'
import { openExternal } from '../../../utils/openExternal'
import { isDesktopTauri } from '../../../utils/platform'
import {
  useAutomaticUpdates,
  useUpdateActions,
  useUpdateConnection,
  useUpdateStatus,
} from '../queries/useUpdateQueries'

export function DesktopUpdates(): ReactNode {
  return isDesktopTauri() ? <NativeDesktopUpdates /> : null
}

function NativeDesktopUpdates(): ReactNode {
  const client = useQueryClient()
  const query = useUpdateStatus()
  const automatic = useAutomaticUpdates()
  const action = useUpdateActions()
  const connection = useUpdateConnection()
  const confirming = useRef(false)
  const status = query.data

  const install = () => {
    const version = status?.available_version
    if (!version || !status.can_install || !connection.connected || action.isPending || confirming.current) {
      return
    }
    confirming.current = true
    openConfirm({
      title: `Install Pipedash ${version}?`,
      children: 'Pipedash will restart. Save any unsaved changes before continuing.',
      labels: { cancel: 'Cancel', confirm: 'Install and restart' },
      onClose: () => {
        confirming.current = false
      },
      onCancel: () => {
        confirming.current = false
      },
      onConfirm: () => {
        confirming.current = false
        const current = client.getQueryData<UpdateStatus>(queryKeys.updates.status())
        if (current?.available_version !== version) {
          displayErrorNotification(
            new Error('A different update is ready. Review it before installing.'),
            'Update changed'
          )
          return
        }
        if (current.can_install) {
          action.submit({ type: 'install', version })
        }
      },
    })
  }
  const openRelease = (url: string) => {
    void openExternal(url).catch((error: unknown) => displayErrorNotification(error, "Couldn't open release notes"))
  }

  if (!status) {
    return (
      <Card padding="md">
        <Stack gap="sm">
          <Text size="sm" fw={600}>
            Updates
          </Text>
          {query.error ? (
            <>
              <Text size="sm" c="red" role="alert">
                {errorMessage(query.error, "Couldn't read update status")}
              </Text>
              <Button variant="light" onClick={() => void query.refetch()} loading={query.isFetching}>
                Retry
              </Button>
            </>
          ) : (
            <LoadingState variant="compact" message="Reading update status…" />
          )}
        </Stack>
      </Card>
    )
  }

  const version = status.available_version
  const knownLength = status.total_bytes !== null && status.total_bytes > 0
  const percentage = knownLength
    ? Math.min(100, (status.downloaded_bytes / (status.total_bytes ?? 1)) * 100)
    : undefined
  const downloading = status.phase === 'downloading'
  const installation = ['preparing', 'installing', 'restarting'].includes(status.phase)
  const heading =
    status.phase === 'ready' && version
      ? `Pipedash ${version} is ready`
      : version
        ? `Pipedash ${version} is available`
        : status.phase === 'checking'
          ? 'Checking for updates…'
          : status.last_checked_at && !status.error
            ? "You're up to date"
            : `Pipedash ${status.current_version}`
  const retryCheck = status.error?.stage === 'check' || status.error?.stage === 'storage'
  const retryDownload = status.error?.stage === 'download'
  const retryInstall = status.error?.stage === 'install'
  const switchChecked = automatic.isPending ? automatic.variables : status.automatic_updates

  return (
    <Card padding="md" aria-label="Updates">
      <Stack gap="md">
        <Stack gap={4}>
          <Text size="sm" fw={600}>
            Updates
          </Text>
          <Text size="sm">{heading}</Text>
          {(version || status.last_checked_at) && (
            <Text size="xs" c="dimmed">
              Installed: Pipedash {status.current_version}
            </Text>
          )}
          <Text size="xs" c="dimmed">
            {status.install_source}
          </Text>
          {status.reason && (
            <Text size="sm" c="dimmed">
              {status.reason}
            </Text>
          )}
          {status.last_checked_at && (
            <Text size="xs" c="dimmed">
              Last checked: {new Date(status.last_checked_at).toLocaleString()}
            </Text>
          )}
        </Stack>

        {downloading && (
          <Stack gap="xs">
            <Progress.Root
              role="progressbar"
              aria-label="Downloading update"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={percentage}
              aria-valuetext={
                knownLength
                  ? `${formatBytes(status.downloaded_bytes)} of ${formatBytes(status.total_bytes ?? 0)}`
                  : `${formatBytes(status.downloaded_bytes)} downloaded; total size unknown`
              }
            >
              <Progress.Section value={percentage ?? 100} animated={!knownLength} withAria={false} aria-hidden />
            </Progress.Root>
            <Text size="xs" c="dimmed">
              {formatBytes(status.downloaded_bytes)}
              {knownLength ? ` / ${formatBytes(status.total_bytes ?? 0)}` : ' downloaded · total size unknown'}
            </Text>
          </Stack>
        )}

        {status.error && (
          <Stack gap="xs">
            <Text size="sm" c="red" role="alert">
              {status.error.message}
            </Text>
            {status.error.stage === 'recovery' && (
              <>
                <Text size="sm">Reinstall Pipedash before quitting. Keep the backup shown above.</Text>
                <Button
                  variant="light"
                  onClick={() => openRelease('https://github.com/hcavarsan/pipedash/releases/latest')}
                >
                  Download Pipedash
                </Button>
              </>
            )}
          </Stack>
        )}
        {automatic.error && (
          <Text size="sm" c="red" role="alert">
            {errorMessage(automatic.error, "Couldn't save automatic updates")}
          </Text>
        )}
        {!connection.connected && status.can_install && (
          <Text size="sm" c="dimmed">
            Connect to update progress before installing.
          </Text>
        )}
        <LiveStatus message={downloading ? 'Downloading update' : (status.error?.message ?? heading)} />

        <Group gap="sm">
          {status.can_check && (
            <Button variant="light" disabled={action.isPending} onClick={() => action.submit({ type: 'check' })}>
              {retryCheck ? 'Retry check' : 'Check for updates'}
            </Button>
          )}
          {status.can_download && (
            <Button disabled={action.isPending} onClick={() => action.submit({ type: 'download' })}>
              {retryDownload ? 'Retry download' : 'Download update'}
            </Button>
          )}
          {status.can_install && (
            <Button disabled={!connection.connected || action.isPending} onClick={install}>
              {retryInstall ? 'Retry install and restart' : 'Install and restart'}
            </Button>
          )}
          {status.release_url && (
            <Button variant="subtle" onClick={() => status.release_url && openRelease(status.release_url)}>
              {status.mode === 'external' ? 'View release' : 'Release notes'}
            </Button>
          )}
        </Group>

        {status.mode !== 'unsupported' && (
          <>
            <Divider />
            <Switch
              label={status.mode === 'native' ? 'Download updates automatically' : 'Check for updates automatically'}
              description={
                status.mode === 'native'
                  ? 'Checks every six hours. Installation and restart always need your approval.'
                  : 'Checks every six hours. Your package manager installs updates; a new release may not be available there yet.'
              }
              checked={switchChecked}
              disabled={automatic.isPending || installation}
              onChange={(event) => automatic.mutate(event.currentTarget.checked)}
            />
          </>
        )}
      </Stack>
    </Card>
  )
}
