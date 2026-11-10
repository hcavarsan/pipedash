import { Alert, Button, Divider, Stack, Text } from '@mantine/core'
import { IconAlertTriangle, IconQrcode } from '@tabler/icons-react'
import type * as BarcodeScanner from '@tauri-apps/plugin-barcode-scanner'
import { type ReactNode, startTransition, useActionState, useRef, useState } from 'react'

import { FormErrorDisplay } from '../../../components/common/FormErrorDisplay'
import { SubmitButton } from '../../../components/forms/SubmitButton'
import { useBlurValidation } from '../../../hooks/useBlurValidation'
import { logger } from '../../../lib/logger'
import { getErrorCode } from '../../../types/errors'
import { errorMessage } from '../../../utils/errorDisplay'
import { useFocusFirstInvalid } from '../../../utils/formFocus'
import { isMobilePlatform } from '../../../utils/platform'
import { sameJson } from '../../../utils/sameJson'
import { usePairAlertHost, usePairWithServer } from '../queries/useConnectionQueries'
import { ConnectServerDone } from './ConnectServerDone'
import { ConnectServerFields } from './ConnectServerFields'

interface Submitted {
  address: string
  code: string
}

interface ConnectActionState {
  addressError: string | null
  codeError: string | null
  globalError: string | null
  errorCode: string | null
  host: string | null
  submitted: Submitted | null
}

type FieldKey = 'address' | 'code'

const INITIAL_STATE: ConnectActionState = {
  addressError: null,
  codeError: null,
  globalError: null,
  errorCode: null,
  host: null,
  submitted: null,
}
const CONNECT_FAILED = 'Check the address and the code, then try again.'
const ADDRESS_REQUIRED = 'Enter the server address or pairing link.'
const CODE_REQUIRED = 'Enter the 8-character code.'
const PRIVATE_HTTP_DISABLED = 'PRIVATE_HTTP_DISABLED'
const LINK_SCHEME = 'pipedash://'
const FIELDSET_STYLE = { border: 0, padding: 0, margin: 0, minInlineSize: 0, display: 'contents' } as const

function fieldValue(formData: FormData, name: string): string {
  const value = formData.get(name)

  return typeof value === 'string' ? value : ''
}

function validateField(key: FieldKey, value: string, isLink: boolean): string | undefined {
  if (key === 'address') {
    return value.trim() ? undefined : ADDRESS_REQUIRED
  }

  return isLink || value.trim() ? undefined : CODE_REQUIRED
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
  const formRef = useRef<HTMLFormElement>(null)
  const focusFirstInvalid = useFocusFirstInvalid(formRef)

  const connectAction = async (_previous: ConnectActionState, formData: FormData): Promise<ConnectActionState> => {
    const submittedAddress = fieldValue(formData, 'address').trim()
    const submittedCode = fieldValue(formData, 'code').trim()
    const isLink = submittedAddress.startsWith(LINK_SCHEME)
    const submitted: Submitted = { address: submittedAddress, code: isLink ? '' : submittedCode }
    const addressError = validateField('address', submittedAddress, isLink) ?? null
    const codeError = validateField('code', submittedCode, isLink) ?? null

    if (addressError || codeError) {
      focusFirstInvalid()

      return { ...INITIAL_STATE, addressError, codeError, submitted }
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

      return { ...INITIAL_STATE, globalError: errorMessage(err, CONNECT_FAILED), errorCode, submitted }
    }
  }

  const isLink = address.trim().startsWith(LINK_SCHEME)
  const draft: Submitted = { address: address.trim(), code: isLink ? '' : code.trim() }
  const validation = useBlurValidation<FieldKey>((key, value) => validateField(key, value, isLink))
  const [state, formAction, pending] = useActionState(connectAction, INITIAL_STATE)
  const visible = state === dismissed || !sameJson(state.submitted, draft) ? INITIAL_STATE : state
  const needsEncryptionConsent = state.errorCode === PRIVATE_HTTP_DISABLED && state.submitted?.address === draft.address

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
      setAllowPrivateHttp(false)
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
    setAllowPrivateHttp(false)
    validation.onChange('address', value)
  }

  const changeCode = (value: string): void => {
    setCode(value)
    validation.onChange('code', value)
  }

  if (state.host) {
    return <ConnectServerDone host={state.host} alerts={alerts} mobileShell={mobileShell} />
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

      <form ref={formRef} noValidate action={formAction}>
        <fieldset disabled={pending} style={FIELDSET_STYLE}>
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
              addressError={validation.errors.address ?? visible.addressError}
              codeError={validation.errors.code ?? visible.codeError}
              needsEncryptionConsent={needsEncryptionConsent}
              allowPrivateHttp={allowPrivateHttp}
              onAddressChange={changeAddress}
              onCodeChange={changeCode}
              onAddressBlur={() => validation.onBlur('address', address)}
              onCodeBlur={() => validation.onBlur('code', code)}
              onAllowPrivateHttpChange={setAllowPrivateHttp}
            />

            <SubmitButton fullWidth size={mobileShell ? 'lg' : 'md'} disabled={scanning}>
              Connect
            </SubmitButton>
          </Stack>
        </fieldset>
      </form>
    </Stack>
  )
}
