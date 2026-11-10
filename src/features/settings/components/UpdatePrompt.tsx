import { Button, Group, Progress, Stack, Text } from '@mantine/core'
import { type ReactNode, useState } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import { useInstallUpdate, useUpdateStatus } from '../queries/useUpdaterQueries'

export function UpdatePrompt(): ReactNode {
  const { data: status } = useUpdateStatus()
  const install = useInstallUpdate()
  const [dismissed, setDismissed] = useState<string | null>(null)

  if (!status?.version) {
    return null
  }
  const { version, phase } = status
  const busy = phase === 'downloading' || phase === 'installing'
  const later = (): void => setDismissed(version)

  return (
    <StandardModal
      opened={busy || (version !== dismissed && (phase === 'available' || phase === 'failed'))}
      onClose={later}
      title="Update available"
      size="md"
      closable={!busy}
      footer={
        <Group justify="flex-end" gap="sm">
          <Button variant="default" size="sm" onClick={later} disabled={busy}>
            Later
          </Button>
          <Button size="sm" onClick={() => install.mutate()} loading={install.isPending} disabled={busy}>
            Update and restart
          </Button>
        </Group>
      }
    >
      <Stack gap="sm">
        <Text size="sm" role="status">
          {status.message}
        </Text>
        <Text size="xs" c="dimmed">
          You have {status.current_version}.
        </Text>
        {status.notes && (
          <Text size="sm" style={{ whiteSpace: 'pre-wrap' }}>
            {status.notes}
          </Text>
        )}
        {phase === 'downloading' && (
          <Progress
            aria-label="Download progress"
            value={status.total ? (status.downloaded / status.total) * 100 : 100}
            animated={!status.total}
          />
        )}
      </Stack>
    </StandardModal>
  )
}
