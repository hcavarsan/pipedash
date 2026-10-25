import { Alert, Button, Card, Divider, PasswordInput, Stack, Text, TextInput } from '@mantine/core'
import { IconInfoCircle } from '@tabler/icons-react'
import { useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useNow } from '../../hooks/useNow'
import { usePairPushServer, usePushSendTest, usePushStatus, useUnpairPushServer } from '../../queries/usePushQueries'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { isHttpUrl } from '../../utils/openExternal'
import { notificationProblem } from '../../utils/systemNotifications'
import { ErrorState } from '../common/ErrorState'
import { IosGroup, IosRow } from '../ios/IosList'

import { AccentText, RowTextInput } from './mobileParts'
import { PushDeviceRow } from './PushDeviceRow'

export function PushNotificationsCard() {
  const { data: status, error, refetch, isRefetching } = usePushStatus()
  const mobileShell = useMobileShell()
  const pair = usePairPushServer()
  const unpair = useUnpairPushServer()
  const sendTest = usePushSendTest()
  const now = useNow(60_000)
  const [serverUrl, setServerUrl] = useState('')
  const [apiToken, setApiToken] = useState('')
  const [label, setLabel] = useState('')

  if (error) {
    return (
      <ErrorState title="Couldn't load push settings" error={error} onRetry={() => refetch()} retrying={isRefetching} />
    )
  }

  if (!status?.supported) {
    return null
  }

  const denied = status.permission === 'denied'
  const canPair = status.relay_configured && !denied
  const canSubmit = isHttpUrl(serverUrl.trim()) && apiToken.length > 0

  const handlePair = () =>
    pair.mutate(
      { serverUrl: serverUrl.trim(), apiToken, label: label.trim() || undefined },
      {
        onSuccess: () => {
          setServerUrl('')
          setApiToken('')
          setLabel('')
        },
      }
    )

  if (mobileShell) {
    const description =
      'Get a push on this device when a pipeline fails on one of your Pipedash servers, even with the app closed.'

    return (
      <Stack gap="lg">
        <IosGroup
          header="Push notifications"
          footer={
            status.relay_configured && status.permission === 'prompt'
              ? `${description} iOS asks for notification permission when you pair your first server.`
              : description
          }
        >
          {!status.relay_configured && (
            <IosRow
              title="Push unavailable"
              subtitle="This build has no push relay configured, so push notifications are unavailable."
            />
          )}
          {status.relay_configured && denied && (
            <IosRow title="Notifications blocked" subtitle={notificationProblem('blocked')} />
          )}
          {canPair && (
            <>
              <IosRow
                title="Server URL"
                accessory={
                  <RowTextInput
                    aria-label="Server URL"
                    placeholder="https://pipedash.example.com"
                    type="url"
                    inputMode="url"
                    value={serverUrl}
                    onChange={(e) => setServerUrl(e.currentTarget.value)}
                  />
                }
              />
              <IosRow
                title="API token"
                accessory={
                  <RowTextInput
                    aria-label="API token of that server"
                    placeholder="Required"
                    type="password"
                    value={apiToken}
                    onChange={(e) => setApiToken(e.currentTarget.value)}
                  />
                }
              />
              <IosRow
                title="Label"
                accessory={
                  <RowTextInput
                    aria-label="Label"
                    placeholder="Optional"
                    autoCapitalize="words"
                    value={label}
                    onChange={(e) => setLabel(e.currentTarget.value)}
                  />
                }
              />
            </>
          )}
        </IosGroup>

        {canPair && (
          <IosGroup>
            <IosRow
              title={<AccentText>{pair.isPending ? 'Pairing...' : 'Pair server'}</AccentText>}
              disabled={!canSubmit || pair.isPending}
              onClick={handlePair}
            />
          </IosGroup>
        )}

        {status.servers.length > 0 && (
          <IosGroup header="Paired servers">
            {status.servers.map((server) => {
              const name = server.label || server.url

              return (
                <PushDeviceRow
                  key={server.id}
                  title={name}
                  subtitle={`${server.url} · paired ${formatRelativeTime(server.paired_at, now)}`}
                  testLabel={`Send a test push from ${name}`}
                  removeLabel={`Remove ${name}`}
                  testing={sendTest.isPending && sendTest.variables === server.id}
                  removing={unpair.isPending && unpair.variables === server.id}
                  onTest={() => sendTest.mutate(server.id)}
                  onRemove={() => unpair.mutate(server.id)}
                />
              )
            })}
          </IosGroup>
        )}
      </Stack>
    )
  }

  return (
    <Card padding="md">
      <Stack gap="md">
        <Stack gap={0}>
          <Text size="sm" fw={600}>
            Push notifications
          </Text>
          <Text size="xs" c="dimmed">
            Get a push on this device when a pipeline fails on one of your Pipedash servers, even with the app closed.
          </Text>
        </Stack>

        {!status.relay_configured && (
          <Alert icon={<IconInfoCircle size={16} />} color="gray" variant="light">
            This build has no push relay configured, so push notifications are unavailable.
          </Alert>
        )}

        {status.relay_configured && denied && (
          <Alert icon={<IconInfoCircle size={16} />} color="orange" variant="light">
            {notificationProblem('blocked')}
          </Alert>
        )}

        {status.relay_configured && status.permission === 'prompt' && (
          <Text size="xs" c="dimmed">
            iOS asks for notification permission when you pair your first server.
          </Text>
        )}

        {canPair && (
          <Stack gap="sm">
            <TextInput
              label="Server URL"
              placeholder="https://pipedash.example.com"
              type="url"
              inputMode="url"
              autoCapitalize="none"
              autoCorrect="off"
              value={serverUrl}
              onChange={(e) => setServerUrl(e.currentTarget.value)}
            />
            <PasswordInput
              label="API token"
              description="The API token of that server."
              value={apiToken}
              onChange={(e) => setApiToken(e.currentTarget.value)}
            />
            <TextInput
              label="Label"
              placeholder="Optional, for example Work"
              value={label}
              onChange={(e) => setLabel(e.currentTarget.value)}
            />
            <Button onClick={handlePair} loading={pair.isPending} disabled={!canSubmit}>
              Pair server
            </Button>
          </Stack>
        )}

        {status.servers.length > 0 && (
          <>
            <Divider />
            <Stack gap="sm">
              {status.servers.map((server) => {
                const name = server.label || server.url

                return (
                  <PushDeviceRow
                    key={server.id}
                    title={name}
                    subtitle={`${server.url} · paired ${formatRelativeTime(server.paired_at, now)}`}
                    testLabel={`Send a test push from ${name}`}
                    removeLabel={`Remove ${name}`}
                    testing={sendTest.isPending && sendTest.variables === server.id}
                    removing={unpair.isPending && unpair.variables === server.id}
                    onTest={() => sendTest.mutate(server.id)}
                    onRemove={() => unpair.mutate(server.id)}
                  />
                )
              })}
            </Stack>
          </>
        )}
      </Stack>
    </Card>
  )
}
