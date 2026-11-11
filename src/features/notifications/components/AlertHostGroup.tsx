import { Button, Card, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import type { AlertHostInfo } from '@/types/notifications'

import { IosGroup, IosRow } from '../../../components/ios/IosList'

const CONNECT_PATH = '/connect?purpose=alerts'
const CONNECT_SUBTITLE =
  "Get alerts from a Pipedash server or a computer that shares Pipedash, even when its pipelines aren't on this device."
const DUPLICATE_NOTE = 'If the same providers are also on this device, you can get the same alert twice.'

interface AlertHostGroupProps {
  mobileShell: boolean
  alertHost: AlertHostInfo | null
  removing: boolean
  onRemove: (name: string) => void
}

export function AlertHostGroup({ mobileShell, alertHost, removing, onRemove }: AlertHostGroupProps): ReactNode {
  const status = alertHost?.push ? 'Push alerts on' : 'Checks in the background'

  if (mobileShell) {
    return (
      <IosGroup header="Alerts from a server" footer={alertHost ? DUPLICATE_NOTE : undefined}>
        {alertHost ? (
          <>
            <IosRow title={alertHost.name} subtitle={status} />
            <IosRow title="Remove" destructive disabled={removing} onClick={() => onRemove(alertHost.name)} />
          </>
        ) : (
          <IosRow title="Connect a server" subtitle={CONNECT_SUBTITLE} to={CONNECT_PATH} />
        )}
      </IosGroup>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="sm" align="flex-start">
        <Stack gap={0}>
          <Text size="sm" fw={500}>
            {alertHost ? alertHost.name : 'Alerts from a server'}
          </Text>
          <Text size="xs" c="dimmed">
            {alertHost ? status : CONNECT_SUBTITLE}
          </Text>
        </Stack>
        {alertHost ? (
          <>
            <Button variant="default" color="red" loading={removing} onClick={() => onRemove(alertHost.name)}>
              Remove
            </Button>
            <Text size="xs" c="dimmed">
              {DUPLICATE_NOTE}
            </Text>
          </>
        ) : (
          <Button component={Link} to={CONNECT_PATH} variant="default">
            Connect a server
          </Button>
        )}
      </Stack>
    </Card>
  )
}
