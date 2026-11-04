import { Alert, Box, Button, Center, Checkbox, Divider, Paper, Stack, Text, TextInput, Title } from '@mantine/core'
import { IconAlertTriangle, IconArrowLeft, IconChevronLeft, IconCircleCheck, IconQrcode } from '@tabler/icons-react'
import { useQueryClient } from '@tanstack/react-query'
import type * as BarcodeScanner from '@tauri-apps/plugin-barcode-scanner'
import { type ReactNode, startTransition, useActionState, useState } from 'react'
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom'

import { AppLogo } from '../../../components/atoms/AppLogo'
import { DocumentTitle } from '../../../components/common/DocumentTitle'
import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { SubmitButton } from '../../../components/forms/SubmitButton'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { withBasePath } from '../../../lib/basePath'
import { logger } from '../../../lib/logger'
import { queryKeys } from '../../../lib/queryKeys'
import { getErrorCode } from '../../../types/errors'
import { errorMessage } from '../../../utils/errorDisplay'
import { isRecord } from '../../../utils/isRecord'
import { isMobilePlatform } from '../../../utils/platform'
import { Onboarding } from '../../setup/components/SetupWizard/Onboarding'
import { usePairAlertHost, usePairWithServer } from '../queries/useConnectionQueries'

interface ConnectActionState {
  addressError: string | null
  codeError: string | null
  globalError: string | null
  errorCode: string | null
  host: string | null
}

const INITIAL_STATE: ConnectActionState = {
  addressError: null,
  codeError: null,
  globalError: null,
  errorCode: null,
  host: null,
}
const CONNECT_FAILED = 'Check the address and the code, then try again.'
const PRIVATE_HTTP_DISABLED = 'PRIVATE_HTTP_DISABLED'
const LINK_SCHEME = 'pipedash://'
const TITLE = 'Connect to a server'
const ALERTS_TITLE = 'Get alerts from a server'

function fieldValue(formData: FormData, name: string): string {
  const value = formData.get(name)

  return typeof value === 'string' ? value : ''
}

interface ConnectFormProps {
  mobileShell: boolean
  link: string
  alerts: boolean
}

function ConnectForm({ mobileShell, link, alerts }: ConnectFormProps): ReactNode {
  const [address, setAddress] = useState(link)
  const [code, setCode] = useState('')
  const [allowPrivateHttp, setAllowPrivateHttp] = useState(false)
  const [cameraDenied, setCameraDenied] = useState(false)
  const [cameraFailed, setCameraFailed] = useState(false)
  const [scanning, setScanning] = useState(false)
  const [dismissed, setDismissed] = useState<ConnectActionState | null>(null)
  const pairConnection = usePairWithServer()
  const pairAlerts = usePairAlertHost()
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  const connectAction = async (_previous: ConnectActionState, formData: FormData): Promise<ConnectActionState> => {
    const submittedAddress = fieldValue(formData, 'address').trim()
    const submittedCode = fieldValue(formData, 'code').trim()
    const isLink = submittedAddress.startsWith(LINK_SCHEME)
    const addressError = submittedAddress ? null : 'Enter the server address or pairing link.'
    const codeError = isLink || submittedCode ? null : 'Enter the 8-character code.'

    if (addressError || codeError) {
      return { ...INITIAL_STATE, addressError, codeError }
    }

    try {
      const input = { address: submittedAddress, code: isLink ? null : submittedCode, allowPrivateHttp }
      const host = alerts
        ? (await pairAlerts.mutateAsync(input)).name
        : await pairConnection.mutateAsync(input).then((info) => info.host ?? info.serverUrl ?? 'the server')

      return { ...INITIAL_STATE, host }
    } catch (err) {
      const errorCode = getErrorCode(err) ?? null

      if (errorCode !== PRIVATE_HTTP_DISABLED) {
        setAllowPrivateHttp(false)
      }

      return { ...INITIAL_STATE, globalError: errorMessage(err, CONNECT_FAILED), errorCode }
    }
  }

  const [state, formAction, pending] = useActionState(connectAction, INITIAL_STATE)
  const visible = state === dismissed ? INITIAL_STATE : state
  const needsEncryptionConsent = state.errorCode === PRIVATE_HTTP_DISABLED
  const isLink = address.trim().startsWith(LINK_SCHEME)

  const scanCode = async (): Promise<void> => {
    setScanning(true)
    setCameraFailed(false)

    try {
      let scanner: typeof BarcodeScanner
      let permission: string

      try {
        scanner = await import('@tauri-apps/plugin-barcode-scanner')
        permission = await scanner.checkPermissions()

        if (permission !== 'granted') {
          permission = await scanner.requestPermissions()
        }
      } catch (error) {
        logger.warn('ConnectServerScreen', "Couldn't check camera access", error)
        setCameraFailed(true)

        return
      }

      setCameraDenied(permission !== 'granted')

      if (permission !== 'granted') {
        return
      }

      let content: string | undefined

      try {
        const scanned = await scanner.scan({ formats: [scanner.Format.QRCode] })
        content = scanned?.content
      } catch {
        return
      }

      if (!content) {
        return
      }

      const formData = new FormData()
      formData.set('address', content)
      formData.set('code', '')
      setAddress(content)
      setCode('')
      startTransition(() => formAction(formData))
    } finally {
      setScanning(false)
    }
  }

  const openSettings = (): void => {
    import('@tauri-apps/plugin-barcode-scanner')
      .then((scanner) => scanner.openAppSettings())
      .catch((error: unknown) => logger.warn('ConnectServerScreen', "Couldn't open Settings", error))
  }

  if (state.host) {
    const done = alerts
      ? {
          title: `Alerts from ${state.host} are on`,
          text: 'This device keeps its own pipelines and also gets alerts from that server.',
          label: 'Done',
          onClick: () => {
            void queryClient.invalidateQueries({ queryKey: queryKeys.localNotifications.settings() })
            void navigate('/settings/notifications', { replace: true })
          },
        }
      : {
          title: `Connected to ${state.host}`,
          text: `This device now shows the pipelines from ${state.host}.`,
          label: 'Open dashboard',
          onClick: () => window.location.replace(withBasePath('/pipelines')),
        }

    return (
      <Stack align="center" gap="md" ta="center">
        <Box c="green" lh={0}>
          <IconCircleCheck size={mobileShell ? 64 : 48} aria-hidden />
        </Box>
        <Stack gap="2xs" align="center">
          <Title order={2} size="h3">
            {done.title}
          </Title>
          <Text size="sm" c="dimmed" style={{ overflowWrap: 'anywhere' }}>
            {done.text}
          </Text>
        </Stack>
        <Button fullWidth size={mobileShell ? 'lg' : 'md'} onClick={done.onClick}>
          {done.label}
        </Button>
      </Stack>
    )
  }

  return (
    <Stack gap={mobileShell ? 'md' : 'lg'}>
      {isMobilePlatform() && (
        <Stack gap="md">
          <Button
            fullWidth
            size="lg"
            leftSection={<IconQrcode size={16} aria-hidden />}
            loading={scanning}
            disabled={pending}
            onClick={() => void scanCode()}
          >
            Scan pairing code
          </Button>
          <Text size="sm" c="dimmed" ta="center">
            On the other device, open Settings → Devices and choose Add device.
          </Text>
          {cameraDenied && (
            <Alert color="orange" icon={<IconAlertTriangle size={16} aria-hidden />} title="Camera access is off">
              <Stack gap="xs" align="flex-start">
                <Text size="sm">Allow camera access in Settings, or enter the code below.</Text>
                <Button size="sm" variant="light" color="orange" onClick={openSettings}>
                  Open Settings
                </Button>
              </Stack>
            </Alert>
          )}
          {cameraFailed && (
            <Alert color="orange" icon={<IconAlertTriangle size={16} aria-hidden />} title="Couldn't open the camera">
              <Text size="sm">Enter the server address and code below.</Text>
            </Alert>
          )}
          <Divider label="or enter it" />
        </Stack>
      )}

      <form noValidate action={formAction}>
        <Stack gap={mobileShell ? 'md' : 'lg'}>
          <FormErrorDisplay
            title="Couldn't connect"
            globalError={needsEncryptionConsent ? null : visible.globalError}
            onDismiss={() => setDismissed(state)}
          />

          <Stack gap={mobileShell ? 'sm' : 'md'}>
            <TextInput
              name="address"
              label="Server address"
              description="You can also paste the pairing link."
              placeholder="https://pipedash.example.com"
              required
              value={address}
              onChange={(event) => {
                setAddress(event.currentTarget.value)
                setDismissed(state)
              }}
              error={visible.addressError}
              size="md"
              autoComplete="off"
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
              enterKeyHint={isLink ? 'go' : 'next'}
            />

            {!isLink && (
              <TextInput
                name="code"
                label="Pairing code"
                placeholder="ABCD-EFGH"
                required
                value={code}
                onChange={(event) => {
                  setCode(event.currentTarget.value)
                  setDismissed(state)
                }}
                error={visible.codeError}
                size="md"
                autoComplete="off"
                enterKeyHint="go"
              />
            )}
          </Stack>

          {needsEncryptionConsent && (
            <Alert
              color="orange"
              icon={<IconAlertTriangle size={16} aria-hidden />}
              title="This server doesn't use encryption"
            >
              <Checkbox
                label="Allow unencrypted connections on private networks"
                description="Anyone on this network could read your pipelines and this device's access token. Turn it on only for a network you trust."
                checked={allowPrivateHttp}
                onChange={(event) => setAllowPrivateHttp(event.currentTarget.checked)}
              />
            </Alert>
          )}

          <SubmitButton fullWidth size={mobileShell ? 'lg' : 'md'} disabled={scanning}>
            Connect
          </SubmitButton>
        </Stack>
      </form>
    </Stack>
  )
}

export function ConnectServerScreen(): ReactNode {
  const navigate = useNavigate()
  const location = useLocation()
  const mobileShell = useMobileShell()
  const alerts = useSearchParams()[0].get('purpose') === 'alerts'
  const title = alerts ? ALERTS_TITLE : TITLE

  const goBack = (): void => {
    if (location.key !== 'default') {
      void navigate(-1)
    } else {
      void navigate('/')
    }
  }

  const link = isRecord(location.state) && typeof location.state.link === 'string' ? location.state.link : ''
  const form = <ConnectForm key={link} mobileShell={mobileShell} link={link} alerts={alerts} />

  if (mobileShell) {
    return (
      <Onboarding
        title={title}
        footer={
          <Button
            fullWidth
            size="lg"
            variant="subtle"
            leftSection={<IconChevronLeft size={16} aria-hidden />}
            onClick={goBack}
          >
            Back
          </Button>
        }
      >
        <DocumentTitle title={title} />
        {form}
      </Onboarding>
    )
  }

  return (
    <Box component="main" h="100vh" className="safe-area">
      <DocumentTitle title={title} />
      <Center h="100%" p={{ base: 'md', sm: 'xl' }}>
        <Paper p={{ base: 'lg', sm: 'xl' }} w="100%" maw={480} withBorder>
          <Stack gap="xl">
            <Stack gap="md" align="center">
              <AppLogo size={{ base: 48, sm: 56 }} alt="Pipedash" />
              <Title order={1} size="h2" ta="center">
                {title}
              </Title>
            </Stack>

            {form}

            <Button
              variant="subtle"
              color="gray"
              leftSection={<IconArrowLeft size={16} aria-hidden />}
              onClick={goBack}
              style={{ alignSelf: 'flex-start' }}
            >
              Back
            </Button>
          </Stack>
        </Paper>
      </Center>
    </Box>
  )
}
