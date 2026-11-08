import { Alert, Box, Button, Divider, Stack, Text, Title } from '@mantine/core'
import { IconAlertTriangle, IconCircleCheck, IconQrcode } from '@tabler/icons-react'
import { useQueryClient } from '@tanstack/react-query'
import type * as BarcodeScanner from '@tauri-apps/plugin-barcode-scanner'
import { type ReactNode, startTransition, useActionState, useState } from 'react'
import { useNavigate } from 'react-router-dom'

import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { SubmitButton } from '../../../components/forms/SubmitButton'
import { withBasePath } from '../../../lib/basePath'
import { logger } from '../../../lib/logger'
import { queryKeys } from '../../../lib/queryKeys'
import { getErrorCode } from '../../../types/errors'
import { errorMessage } from '../../../utils/errorDisplay'
import { isMobilePlatform } from '../../../utils/platform'
import { usePairAlertHost, usePairWithServer } from '../queries/useConnectionQueries'
import { ConnectServerFields } from './ConnectServerFields'

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

function fieldValue(formData: FormData, name: string): string {
  const value = formData.get(name)

  return typeof value === 'string' ? value : ''
}

interface ConnectServerFormProps {
  mobileShell: boolean
  link: string
  alerts: boolean
}

export function ConnectServerForm({ mobileShell, link, alerts }: ConnectServerFormProps): ReactNode {
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

  const changeAddress = (value: string): void => {
    setAddress(value)
    setDismissed(state)
  }

  const changeCode = (value: string): void => {
    setCode(value)
    setDismissed(state)
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

  const cameraDeniedAlert = cameraDenied && (
    <Alert color="orange" icon={<IconAlertTriangle size={16} aria-hidden />} title="Camera access is off">
      <Stack gap="xs" align="flex-start">
        <Text size="sm">Allow camera access in Settings, or enter the code below.</Text>
        <Button size="sm" variant="light" color="orange" onClick={openSettings}>
          Open Settings
        </Button>
      </Stack>
    </Alert>
  )

  const cameraFailedAlert = cameraFailed && (
    <Alert color="orange" icon={<IconAlertTriangle size={16} aria-hidden />} title="Couldn't open the camera">
      <Text size="sm">Enter the server address and code below.</Text>
    </Alert>
  )

  const scanSection = isMobilePlatform() && (
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
      {cameraDeniedAlert}
      {cameraFailedAlert}
      <Divider label="or enter it" />
    </Stack>
  )

  return (
    <Stack gap={mobileShell ? 'md' : 'lg'}>
      {scanSection}

      <form noValidate action={formAction}>
        <Stack gap={mobileShell ? 'md' : 'lg'}>
          <FormErrorDisplay
            title="Couldn't connect"
            globalError={needsEncryptionConsent ? null : visible.globalError}
            onDismiss={() => setDismissed(state)}
          />

          <ConnectServerFields
            mobileShell={mobileShell}
            address={address}
            code={code}
            isLink={isLink}
            addressError={visible.addressError}
            codeError={visible.codeError}
            needsEncryptionConsent={needsEncryptionConsent}
            allowPrivateHttp={allowPrivateHttp}
            onAddressChange={changeAddress}
            onCodeChange={changeCode}
            onAllowPrivateHttpChange={setAllowPrivateHttp}
          />

          <SubmitButton fullWidth size={mobileShell ? 'lg' : 'md'} disabled={scanning}>
            Connect
          </SubmitButton>
        </Stack>
      </form>
    </Stack>
  )
}
