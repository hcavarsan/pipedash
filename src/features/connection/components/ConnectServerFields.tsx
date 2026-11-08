import { Alert, Checkbox, Stack, TextInput } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'
import type { ReactNode } from 'react'

interface ConnectServerFieldsProps {
  mobileShell: boolean
  address: string
  code: string
  isLink: boolean
  addressError: string | null
  codeError: string | null
  needsEncryptionConsent: boolean
  allowPrivateHttp: boolean
  onAddressChange: (value: string) => void
  onCodeChange: (value: string) => void
  onAllowPrivateHttpChange: (value: boolean) => void
}

export function ConnectServerFields({
  mobileShell,
  address,
  code,
  isLink,
  addressError,
  codeError,
  needsEncryptionConsent,
  allowPrivateHttp,
  onAddressChange,
  onCodeChange,
  onAllowPrivateHttpChange,
}: ConnectServerFieldsProps): ReactNode {
  return (
    <>
      <Stack gap={mobileShell ? 'sm' : 'md'}>
        <TextInput
          name="address"
          label="Server address"
          description="You can also paste the pairing link."
          placeholder="https://pipedash.example.com"
          required
          value={address}
          onChange={(event) => onAddressChange(event.currentTarget.value)}
          error={addressError}
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
            onChange={(event) => onCodeChange(event.currentTarget.value)}
            error={codeError}
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
            onChange={(event) => onAllowPrivateHttpChange(event.currentTarget.checked)}
          />
        </Alert>
      )}
    </>
  )
}
