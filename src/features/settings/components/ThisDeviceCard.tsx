import { Button, Card, Stack, Switch, Text } from '@mantine/core'
import { type ReactNode, useState } from 'react'
import { useNavigate } from 'react-router-dom'

import { IosGroup, IosRow } from '@/components/ios/IosList'
import { SwitchRow } from '@/components/ios/mobileParts'
import { confirmDisconnect } from '@/features/connection/confirmDisconnect'
import { useDisconnectServer, useSetAllowPrivateHttp } from '@/features/connection/queries/useConnectionQueries'
import { useMobileShell } from '@/hooks/useIsMobile'
import { getConnection, hasLocalCore, isRemoteClient } from '@/services/connection'
import { errorMessage } from '@/utils/errorDisplay'

const STANDALONE_TEXT = 'Pipedash runs here and checks your providers from this device.'
const TOGGLE_LABEL = 'Allow unencrypted connections on private networks'
const TOGGLE_DESCRIPTION =
  "Needed for addresses that start with http://, such as a computer sharing Pipedash at home. Anyone on that network could read your pipelines and this device's access token. Turn it on only for networks you trust."

export function ThisDeviceCard(): ReactNode {
  const mobileShell = useMobileShell()
  const navigate = useNavigate()
  const disconnect = useDisconnectServer()
  const privateHttp = useSetAllowPrivateHttp()
  const connection = getConnection()
  const [allowPrivateHttp, setAllowPrivateHttp] = useState(connection.allowPrivateHttp ?? false)
  const standalone = hasLocalCore()
  const client = isRemoteClient()
  const host = connection.host ?? ''
  const toggleError = privateHttp.isError
    ? errorMessage(privateHttp.error, "Couldn't change the connection setting")
    : undefined

  const handleDisconnect = async (): Promise<void> => {
    if (await confirmDisconnect(host)) {
      disconnect.mutate()
    }
  }

  const handleToggle = (next: boolean): void => {
    privateHttp.mutate(next, { onSuccess: () => setAllowPrivateHttp(next) })
  }

  if (mobileShell) {
    return (
      <Stack gap="lg">
        {(standalone || client) && (
          <IosGroup>
            {standalone && (
              <>
                <IosRow title={STANDALONE_TEXT} />
                <IosRow title="Connect to a server" onClick={() => navigate('/connect')} chevron />
              </>
            )}
            {client && (
              <>
                <IosRow title={`Shows pipelines from ${host}.`} />
                <IosRow
                  title="Disconnect"
                  destructive
                  disabled={disconnect.isPending}
                  onClick={() => {
                    void handleDisconnect()
                  }}
                />
              </>
            )}
          </IosGroup>
        )}
        <IosGroup footer={toggleError}>
          <SwitchRow
            title={TOGGLE_LABEL}
            subtitle={TOGGLE_DESCRIPTION}
            checked={allowPrivateHttp}
            disabled={privateHttp.isPending}
            onChange={(event) => handleToggle(event.currentTarget.checked)}
          />
        </IosGroup>
      </Stack>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Text size="sm" fw={600}>
          This device
        </Text>

        {standalone && (
          <Stack gap="sm" align="flex-start">
            <Text size="sm">{STANDALONE_TEXT}</Text>
            <Button variant="default" onClick={() => navigate('/connect')}>
              Connect to a server
            </Button>
          </Stack>
        )}

        {client && (
          <Stack gap="sm" align="flex-start">
            <Text size="sm">Shows pipelines from {host}.</Text>
            <Button
              variant="outline"
              color="red"
              loading={disconnect.isPending}
              onClick={() => {
                void handleDisconnect()
              }}
            >
              Disconnect
            </Button>
          </Stack>
        )}

        <Switch
          label={TOGGLE_LABEL}
          description={TOGGLE_DESCRIPTION}
          checked={allowPrivateHttp}
          disabled={privateHttp.isPending}
          error={toggleError}
          onChange={(event) => handleToggle(event.currentTarget.checked)}
        />
      </Stack>
    </Card>
  )
}
