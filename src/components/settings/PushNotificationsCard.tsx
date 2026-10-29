import { Alert, Card, Divider, PasswordInput, Stack, Text, TextInput } from '@mantine/core'
import { IconInfoCircle } from '@tabler/icons-react'
import { type ReactNode, useActionState, useId, useRef, useState } from 'react'

import { useFormEnterNavigation } from '../../hooks/useFormEnterNavigation'
import { useMobileShell } from '../../hooks/useIsMobile'
import { useNow } from '../../hooks/useNow'
import { usePairPushServer, usePushSendTest, usePushStatus, useUnpairPushServer } from '../../queries/usePushQueries'
import type { PushStatus } from '../../types'
import { errorMessage } from '../../utils/errorDisplay'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { isHttpUrl } from '../../utils/openExternal'
import { notificationProblem } from '../../utils/systemNotifications'
import { ErrorState } from '../common/ErrorState'
import { FormErrorDisplay } from '../common/FormErrorDisplay'
import { LiveStatus } from '../forms/LiveStatus'
import { SubmitButton } from '../forms/SubmitButton'
import { IosGroup, IosRow } from '../ios/IosList'

import { FieldRow, RowTextInput } from './mobileParts'
import { NotificationsBlockedRows } from './NotificationsBlockedRows'
import { PushDeviceRow } from './PushDeviceRow'

const URL_ERROR = 'Enter a valid HTTP or HTTPS server URL'
const TOKEN_ERROR = 'API token is required'
const DESCRIPTION =
  'Get a push on this device when a pipeline fails on one of your Pipedash servers, even with the app closed.'

interface PairFieldErrors {
  serverUrl?: string
  apiToken?: string
}

interface PairState {
  error: string | null
  fieldErrors: PairFieldErrors
}

const INITIAL_STATE: PairState = { error: null, fieldErrors: {} }

function fieldValue(formData: FormData, name: string): string {
  const value = formData.get(name)

  return typeof value === 'string' ? value : ''
}

function PushPairingForm({ mobileShell, footer }: { mobileShell: boolean; footer?: string | undefined }): ReactNode {
  const pair = usePairPushServer()
  const formRef = useRef<HTMLFormElement>(null)
  const urlId = useId()
  const tokenId = useId()
  const [serverUrl, setServerUrl] = useState('')
  const [apiToken, setApiToken] = useState('')
  const [label, setLabel] = useState('')
  const [touched, setTouched] = useState({ serverUrl: false, apiToken: false })

  useFormEnterNavigation(formRef)

  const pairAction = async (_previous: PairState, formData: FormData): Promise<PairState> => {
    const url = fieldValue(formData, 'serverUrl').trim()
    const token = fieldValue(formData, 'apiToken')
    const trimmedLabel = fieldValue(formData, 'label').trim()
    const fieldErrors: PairFieldErrors = {}

    setTouched({ serverUrl: true, apiToken: true })

    if (!isHttpUrl(url)) {
      fieldErrors.serverUrl = URL_ERROR
    }

    if (!token) {
      fieldErrors.apiToken = TOKEN_ERROR
    }

    if (fieldErrors.serverUrl || fieldErrors.apiToken) {
      return { error: null, fieldErrors }
    }

    try {
      await pair.mutateAsync({ serverUrl: url, apiToken: token, ...(trimmedLabel ? { label: trimmedLabel } : {}) })
    } catch (error: unknown) {
      return { error: errorMessage(error, 'Check the server URL and token, then try again.'), fieldErrors: {} }
    }

    setServerUrl('')
    setApiToken('')
    setLabel('')
    setTouched({ serverUrl: false, apiToken: false })

    return INITIAL_STATE
  }

  const [state, formAction, isPending] = useActionState(pairAction, INITIAL_STATE)

  const urlError =
    (touched.serverUrl || state.fieldErrors.serverUrl) && !isHttpUrl(serverUrl.trim()) ? URL_ERROR : undefined
  const tokenError = (touched.apiToken || state.fieldErrors.apiToken) && !apiToken ? TOKEN_ERROR : undefined
  const urlProps = {
    name: 'serverUrl',
    value: serverUrl,
    disabled: isPending,
    required: true,
    error: urlError,
    onChange: (event: { currentTarget: { value: string } }) => setServerUrl(event.currentTarget.value),
    onBlur: () => setTouched((value) => ({ ...value, serverUrl: true })),
  }
  const tokenProps = {
    name: 'apiToken',
    value: apiToken,
    disabled: isPending,
    required: true,
    error: tokenError,
    onChange: (event: { currentTarget: { value: string } }) => setApiToken(event.currentTarget.value),
    onBlur: () => setTouched((value) => ({ ...value, apiToken: true })),
  }
  const labelProps = {
    name: 'label',
    value: label,
    disabled: isPending,
    onChange: (event: { currentTarget: { value: string } }) => setLabel(event.currentTarget.value),
  }

  const errors = (
    <>
      <FormErrorDisplay title="Couldn't pair server" globalError={state.error} />
      <LiveStatus message={isPending ? 'Pairing server...' : ''} />
    </>
  )

  if (mobileShell) {
    return (
      <form ref={formRef} noValidate action={formAction}>
        <Stack gap="lg">
          <IosGroup header="Push notifications" footer={footer}>
            <FieldRow title="Server URL">
              {(id) => (
                <RowTextInput
                  id={id}
                  placeholder="https://pipedash.example.com"
                  type="url"
                  inputMode="url"
                  autoComplete="off"
                  enterKeyHint="next"
                  {...urlProps}
                />
              )}
            </FieldRow>
            <FieldRow title="API token">
              {(id) => (
                <RowTextInput
                  id={id}
                  placeholder="Required"
                  type="password"
                  autoComplete="off"
                  enterKeyHint="next"
                  {...tokenProps}
                />
              )}
            </FieldRow>
            <FieldRow title="Label">
              {(id) => (
                <RowTextInput
                  id={id}
                  placeholder="Optional"
                  autoCapitalize="words"
                  autoComplete="off"
                  enterKeyHint="go"
                  {...labelProps}
                />
              )}
            </FieldRow>
          </IosGroup>
          {errors}
          <IosGroup>
            <IosRow
              title={
                <SubmitButton variant="subtle" fullWidth>
                  {isPending ? 'Pairing...' : 'Pair server'}
                </SubmitButton>
              }
            />
          </IosGroup>
        </Stack>
      </form>
    )
  }

  return (
    <form ref={formRef} noValidate action={formAction}>
      <Stack gap="sm">
        <TextInput
          label="Server URL"
          placeholder="https://pipedash.example.com"
          type="url"
          inputMode="url"
          autoCapitalize="none"
          autoCorrect="off"
          id={urlId}
          {...urlProps}
        />
        <PasswordInput label="API token" description="The API token of that server." id={tokenId} {...tokenProps} />
        <TextInput label="Label" placeholder="Optional, for example Work" {...labelProps} />
        {errors}
        <SubmitButton>Pair server</SubmitButton>
      </Stack>
    </form>
  )
}

interface PairedServersProps {
  status: PushStatus
  now: number
  testingId: string | undefined
  removingId: string | undefined
  onTest: (id: string) => void
  onRemove: (id: string) => void
}

function PairedServerRows({ status, now, testingId, removingId, onTest, onRemove }: PairedServersProps): ReactNode {
  return status.servers.map((server) => {
    const name = server.label || server.url

    return (
      <PushDeviceRow
        key={server.id}
        title={name}
        subtitle={`${server.url} · paired ${formatRelativeTime(server.paired_at, now)}`}
        testLabel={`Send a test push from ${name}`}
        removeLabel={`Remove ${name}`}
        testing={testingId === server.id}
        removing={removingId === server.id}
        onTest={() => onTest(server.id)}
        onRemove={() => onRemove(server.id)}
      />
    )
  })
}

export function PushNotificationsCard(): ReactNode {
  const { data: status, error, refetch, isRefetching } = usePushStatus()
  const mobileShell = useMobileShell()
  const unpair = useUnpairPushServer()
  const sendTest = usePushSendTest()
  const now = useNow(60_000)

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
  const prompt = status.relay_configured && status.permission === 'prompt'
  const rows = (
    <PairedServerRows
      status={status}
      now={now}
      testingId={sendTest.isPending ? sendTest.variables : undefined}
      removingId={unpair.isPending ? unpair.variables : undefined}
      onTest={(id) => sendTest.mutate(id)}
      onRemove={(id) => unpair.mutate(id)}
    />
  )

  if (mobileShell) {
    const footer = prompt
      ? `${DESCRIPTION} iOS asks for notification permission when you pair your first server.`
      : DESCRIPTION

    return (
      <Stack gap="lg">
        {canPair ? (
          <PushPairingForm mobileShell footer={footer} />
        ) : (
          <IosGroup header="Push notifications" footer={footer}>
            {!status.relay_configured && (
              <IosRow
                title="Push unavailable"
                subtitle="This build has no push relay configured, so push notifications are unavailable."
              />
            )}
            {status.relay_configured && denied && <NotificationsBlockedRows />}
          </IosGroup>
        )}

        {status.servers.length > 0 && <IosGroup header="Paired servers">{rows}</IosGroup>}
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
            {DESCRIPTION}
          </Text>
        </Stack>

        {!status.relay_configured && (
          <Alert icon={<IconInfoCircle size={16} aria-hidden />} color="gray" variant="light">
            This build has no push relay configured, so push notifications are unavailable.
          </Alert>
        )}

        {status.relay_configured && denied && (
          <Alert icon={<IconInfoCircle size={16} aria-hidden />} color="orange" variant="light">
            {notificationProblem('blocked')}
          </Alert>
        )}

        {prompt && (
          <Text size="xs" c="dimmed">
            iOS asks for notification permission when you pair your first server.
          </Text>
        )}

        {canPair && <PushPairingForm mobileShell={false} />}

        {status.servers.length > 0 && (
          <>
            <Divider />
            <Stack gap="sm">{rows}</Stack>
          </>
        )}
      </Stack>
    </Card>
  )
}
