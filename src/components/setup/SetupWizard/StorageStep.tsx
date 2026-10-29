import { Alert, Card, Code, PasswordInput, SegmentedControl, Stack, Text, TextInput } from '@mantine/core'
import { IconAlertCircle, IconAlertTriangle, IconInfoCircle, IconKey } from '@tabler/icons-react'
import { type ReactNode, useMemo, useState } from 'react'

import { useMobileShell } from '../../../hooks/useIsMobile'
import { isTauri } from '../../../services'
import { isMobilePlatform } from '../../../utils/platform'
import { typedKeys } from '../../../utils/typedKeys'
import { LoadingState } from '../../common/LoadingState'
import { IosGroup, IosRow } from '../../ios/IosList'
import { STORAGE_BACKEND_LABELS } from '../../settings/storageLabels'

import type { StorageBackend, StorageStepProps } from './types'

export function isPostgresUrl(value: string): boolean {
  try {
    const url = new URL(value.trim())

    return (url.protocol === 'postgres:' || url.protocol === 'postgresql:') && Boolean(url.hostname)
  } catch {
    return false
  }
}

function analyzePasswordStrength(password: string): string[] {
  const warnings: string[] = []

  if (password.length < 16) {
    warnings.push('Consider using 16+ characters')
  }
  if (!/\d/.test(password)) {
    warnings.push('Consider adding numbers')
  }
  if (!/[!@#$%^&*()_+\-=[\]{};':"\\|,.<>/?]/.test(password)) {
    warnings.push('Consider adding special characters')
  }
  if (!/[A-Z]/.test(password) || !/[a-z]/.test(password)) {
    warnings.push('Consider mixing uppercase and lowercase')
  }

  return warnings
}

export function StorageStep({
  state,
  setState,
  currentConfig,
  vaultStatus,
  error,
  checkingVault,
}: StorageStepProps): ReactNode {
  const mobileShell = useMobileShell()
  const [blurred, setBlurred] = useState({ postgresUrl: false, password: false, confirmation: false })
  const passwordsMatch = state.vaultPassword === state.vaultPasswordConfirm
  const vaultPasswordFromEnv = vaultStatus?.is_set === true

  const currentBackend = currentConfig?.config.storage.backend

  const isFromKeyring = isTauri() && currentBackend === 'sqlite' && !vaultPasswordFromEnv

  const passwordWarnings = useMemo(() => {
    if (state.vaultPassword.length < 12) {
      return []
    }

    return analyzePasswordStrength(state.vaultPassword)
  }, [state.vaultPassword])

  if (isMobilePlatform()) {
    if (mobileShell) {
      return (
        <Stack gap="md">
          <IosGroup
            header="On-device storage"
            footer="Your data stays in a private database on this device. Provider tokens are kept in the Keychain, so there is no vault password to set."
          >
            <IosRow title="Database" value={STORAGE_BACKEND_LABELS.sqlite} />
            <IosRow title="Provider tokens" value="Device Keychain" />
          </IosGroup>

          {error && (
            <Text size="sm" c="red" px="md" role="alert">
              {error}
            </Text>
          )}
        </Stack>
      )
    }

    return (
      <Stack gap="md">
        <Card p="sm">
          <Stack gap="xs">
            <Text size="sm" fw={600}>
              On-device storage
            </Text>
            <Text size="xs" c="dimmed">
              Pipedash keeps its data in a SQLite database in the app's own storage. Provider tokens stay in the device
              Keychain, so there is no vault password to set.
            </Text>
          </Stack>
        </Card>

        {error && (
          <Alert icon={<IconAlertCircle size={16} />} color="red">
            {error}
          </Alert>
        )}
      </Stack>
    )
  }

  return (
    <Stack gap="md">
      {isFromKeyring && (
        <Alert icon={<IconKey size={16} />} color="orange" title="Credential migration">
          <Text size="sm">
            Your provider credentials are currently stored in the system keyring. After this migration, they will be
            encrypted and stored in the database. You'll need to enter a vault password to access them.
          </Text>
        </Alert>
      )}

      <Card p="sm">
        <Stack gap="sm">
          <Text size="sm" fw={600}>
            Database backend
          </Text>
          <SegmentedControl<StorageBackend>
            aria-label="Database backend"
            value={state.backend}
            onChange={(value) => setState({ ...state, backend: value })}
            data={typedKeys(STORAGE_BACKEND_LABELS).map((value) => ({
              label: STORAGE_BACKEND_LABELS[value],
              value,
            }))}
            fullWidth
          />

          {state.backend === 'postgres' && (
            <TextInput
              label="PostgreSQL connection URL"
              placeholder="postgresql://user:password@localhost:5432/pipedash"
              description="Use postgres:// or postgresql:// followed by your database connection details."
              value={state.postgresUrl}
              onChange={(e) => setState({ ...state, postgresUrl: e.target.value })}
              onBlur={() => setBlurred((value) => ({ ...value, postgresUrl: true }))}
              error={
                blurred.postgresUrl && !isPostgresUrl(state.postgresUrl)
                  ? 'Enter a valid PostgreSQL connection URL'
                  : undefined
              }
              required
            />
          )}

          {state.backend === 'sqlite' && (
            <TextInput
              label="Data directory"
              value={state.dataDir}
              onChange={(e) => setState({ ...state, dataDir: e.target.value })}
            />
          )}

          <Text size="xs" c="dimmed">
            {currentBackend ? `Current backend: ${STORAGE_BACKEND_LABELS[currentBackend]}. ` : ''}
            {state.backend === 'sqlite'
              ? 'Local encrypted storage with vault password'
              : 'Centralized storage, multi-user, encrypted tokens'}
          </Text>
        </Stack>
      </Card>

      {!vaultPasswordFromEnv && (
        <Card p="sm">
          <Stack gap="sm">
            <Text size="sm" fw={600}>
              Vault password
            </Text>
            <Text size="xs" c="dimmed">
              Encrypts your provider tokens. Set <Code>PIPEDASH_VAULT_PASSWORD</Code> env var for auto-unlock.
            </Text>
            {checkingVault ? (
              <LoadingState message="Checking vault…" />
            ) : (
              <>
                <PasswordInput
                  label="Password"
                  placeholder="Min. 8 characters"
                  description="Use at least 8 characters."
                  value={state.vaultPassword}
                  onChange={(e) => setState({ ...state, vaultPassword: e.target.value })}
                  onBlur={() => setBlurred((value) => ({ ...value, password: true }))}
                  error={blurred.password && state.vaultPassword.length < 8 ? 'Use at least 8 characters' : undefined}
                  required
                />
                <PasswordInput
                  label="Confirm password"
                  placeholder="Re-enter password"
                  value={state.vaultPasswordConfirm}
                  onChange={(e) => setState({ ...state, vaultPasswordConfirm: e.target.value })}
                  onBlur={() => setBlurred((value) => ({ ...value, confirmation: true }))}
                  error={blurred.confirmation && !passwordsMatch ? 'Passwords do not match' : undefined}
                  required
                />
                {passwordWarnings.length > 0 && (
                  <Alert icon={<IconAlertTriangle size={14} />} color="orange" variant="light" p="xs">
                    <Text size="xs">{passwordWarnings.join(' · ')}</Text>
                  </Alert>
                )}
              </>
            )}
          </Stack>
        </Card>
      )}

      {vaultPasswordFromEnv && (
        <Alert icon={<IconInfoCircle size={16} />} color="gray" variant="light" p="sm">
          <Text size="sm">
            Vault password configured via <Code>{vaultStatus?.env_var_name}</Code>
          </Text>
        </Alert>
      )}

      {error && (
        <Alert icon={<IconAlertCircle size={16} />} color="red" title="Couldn't save storage settings">
          {error}
        </Alert>
      )}
    </Stack>
  )
}
