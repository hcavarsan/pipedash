import {
  Alert,
  Card,
  Code,
  Group,
  Loader,
  PasswordInput,
  SegmentedControl,
  Stack,
  Text,
  TextInput,
} from '@mantine/core'
import { IconAlertCircle, IconAlertTriangle, IconInfoCircle, IconKey } from '@tabler/icons-react'
import { useMemo } from 'react'

import { isTauri } from '../../../services'
import { isMobilePlatform } from '../../../utils/platform'
import { STORAGE_BACKEND_LABELS } from '../../settings/storageLabels'

import type { StorageBackend, StorageStepProps } from './types'

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

export function StorageStep({ state, setState, currentConfig, vaultStatus, error, checkingVault }: StorageStepProps) {
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
          <SegmentedControl
            aria-label="Database backend"
            value={state.backend}
            onChange={(value) => setState({ ...state, backend: value as StorageBackend })}
            data={(Object.keys(STORAGE_BACKEND_LABELS) as StorageBackend[]).map((value) => ({
              label: STORAGE_BACKEND_LABELS[value],
              value,
            }))}
            fullWidth
          />

          {state.backend === 'postgres' && (
            <TextInput
              label="PostgreSQL connection URL"
              placeholder="postgresql://user:password@localhost:5432/pipedash"
              value={state.postgresUrl}
              onChange={(e) => setState({ ...state, postgresUrl: e.target.value })}
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
            {currentBackend ? `Current backend: ${STORAGE_BACKEND_LABELS[currentBackend as StorageBackend]}. ` : ''}
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
              <Group gap="xs">
                <Loader size="xs" />
                <Text size="sm" c="dimmed">
                  Checking vault...
                </Text>
              </Group>
            ) : (
              <>
                <PasswordInput
                  label="Password"
                  placeholder="Min. 8 characters"
                  value={state.vaultPassword}
                  onChange={(e) => setState({ ...state, vaultPassword: e.target.value })}
                  required
                />
                <PasswordInput
                  label="Confirm password"
                  placeholder="Re-enter password"
                  value={state.vaultPasswordConfirm}
                  onChange={(e) => setState({ ...state, vaultPasswordConfirm: e.target.value })}
                  error={state.vaultPasswordConfirm && !passwordsMatch ? 'Passwords do not match' : undefined}
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
        <Alert icon={<IconAlertCircle size={16} />} color="red">
          {error}
        </Alert>
      )}
    </Stack>
  )
}
