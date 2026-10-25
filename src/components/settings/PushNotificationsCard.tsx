import { Alert, Button, Card, Divider, PasswordInput, Stack, Text, TextInput } from '@mantine/core'
import { IconInfoCircle } from '@tabler/icons-react'
import { type KeyboardEvent, useRef, useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useNow } from '../../hooks/useNow'
import { usePairPushServer, usePushSendTest, usePushStatus, useUnpairPushServer } from '../../queries/usePushQueries'
import { errorMessage } from '../../utils/errorDisplay'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { isHttpUrl } from '../../utils/openExternal'
import { notificationProblem } from '../../utils/systemNotifications'
import { ErrorState } from '../common/ErrorState'
import { FormErrorDisplay } from '../common/FormErrorDisplay'
import { IosGroup, IosRow } from '../ios/IosList'

import { AccentText, FieldRow, RowTextInput } from './mobileParts'
import { NotificationsBlockedRows } from './NotificationsBlockedRows'
import { PushDeviceRow } from './PushDeviceRow'

const submitOnReturn = (event: KeyboardEvent, action: () => void) => {
  if (event.key === 'Enter') {
    event.preventDefault()
    action()
  }
}

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
  const [blurred, setBlurred] = useState({ serverUrl: false, apiToken: false })
  const tokenRef = useRef<HTMLInputElement>(null)
  const labelRef = useRef<HTMLInputElement>(null)

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
          setBlurred({ serverUrl: false, apiToken: false })
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
          {status.relay_configured && denied && <NotificationsBlockedRows />}
          {canPair && (
            <>
              <FieldRow title="Server URL">
                {(id) => (
                  <RowTextInput
                    id={id}
                    placeholder="https://pipedash.example.com"
                    type="url"
                    inputMode="url"
                    autoComplete="off"
                    enterKeyHint="next"
                    value={serverUrl}
                    onChange={(e) => setServerUrl(e.currentTarget.value)}
                    onKeyDown={(e) => submitOnReturn(e, () => tokenRef.current?.focus())}
                  />
                )}
              </FieldRow>
              <FieldRow title="API token">
                {(id) => (
                  <RowTextInput
                    id={id}
                    ref={tokenRef}
                    placeholder="Required"
                    type="password"
                    autoComplete="off"
                    enterKeyHint="next"
                    value={apiToken}
                    onChange={(e) => setApiToken(e.currentTarget.value)}
                    onKeyDown={(e) => submitOnReturn(e, () => labelRef.current?.focus())}
                  />
                )}
              </FieldRow>
              <FieldRow title="Label">
                {(id) => (
                  <RowTextInput
                    id={id}
                    ref={labelRef}
                    placeholder="Optional"
                    autoCapitalize="words"
                    autoComplete="off"
                    enterKeyHint="go"
                    value={label}
                    onChange={(e) => setLabel(e.currentTarget.value)}
                    onKeyDown={(e) =>
                      submitOnReturn(e, () => {
                        if (canSubmit && !pair.isPending) {
                          handlePair()
                        }
                      })
                    }
                  />
                )}
              </FieldRow>
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
          <form
            noValidate
            onSubmit={(event) => {
              event.preventDefault()
              setBlurred({ serverUrl: true, apiToken: true })
              if (canSubmit && !pair.isPending) {
                handlePair()
              }
            }}
          >
            <Stack gap="sm">
              <TextInput
                label="Server URL"
                placeholder="https://pipedash.example.com"
                type="url"
                inputMode="url"
                autoCapitalize="none"
                autoCorrect="off"
                value={serverUrl}
                onChange={(e) => {
                  setServerUrl(e.currentTarget.value)
                  pair.reset()
                }}
                onBlur={() => setBlurred((value) => ({ ...value, serverUrl: true }))}
                error={
                  blurred.serverUrl && !isHttpUrl(serverUrl.trim())
                    ? 'Enter a valid HTTP or HTTPS server URL'
                    : undefined
                }
                required
              />
              <PasswordInput
                label="API token"
                description="The API token of that server."
                value={apiToken}
                onChange={(e) => {
                  setApiToken(e.currentTarget.value)
                  pair.reset()
                }}
                onBlur={() => setBlurred((value) => ({ ...value, apiToken: true }))}
                error={blurred.apiToken && !apiToken ? 'API token is required' : undefined}
                required
              />
              <TextInput
                label="Label"
                placeholder="Optional, for example Work"
                value={label}
                onChange={(e) => setLabel(e.currentTarget.value)}
              />
              <FormErrorDisplay
                title="Couldn't pair server"
                globalError={
                  pair.error ? errorMessage(pair.error, 'Check the server URL and token, then try again.') : null
                }
              />
              <Button type="submit" loading={pair.isPending}>
                Pair server
              </Button>
            </Stack>
          </form>
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
