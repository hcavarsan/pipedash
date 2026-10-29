import { Alert, Card, Code, Group, Stack, Text, Tooltip } from '@mantine/core'
import { IconAlertCircle, IconCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { CopyButton } from '../../../../components/atoms/CopyButton'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import { STORAGE_BACKEND_LABELS } from '../../../../constants/storageLabels'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import { isMobilePlatform, secureStoreLabel } from '../../../../utils/platform'

import type { ConfirmStepProps } from './types'
import { buildVaultExportCommand } from './vaultExportCommand'

const LABEL_WIDTH = 130

const summarizeConnection = (url: string): string | null => {
  try {
    const { host, pathname } = new URL(url)

    return host ? `${host}${pathname}` : null
  } catch {
    return null
  }
}

export function ConfirmStep({
  state,
  showTransferStep,
  vaultPasswordFromEnv,
  isMobile,
  error,
  vaultStatus,
}: ConfirmStepProps): ReactNode {
  const onDevice = isMobilePlatform()
  const mobileShell = useMobileShell()

  const truncatePath = (path: string): string => {
    const segments = path.split('/')

    if (segments.length <= 2) {
      return path
    }

    return `.../${segments.slice(-2).join('/')}`
  }

  const envVarName = vaultStatus.env_var_name || 'PIPEDASH_VAULT_PASSWORD'
  const exportCommand = buildVaultExportCommand(envVarName, state.vaultPassword)
  const showEnvVarSection = !vaultPasswordFromEnv && state.vaultPassword

  if (mobileShell) {
    return (
      <Stack gap="md">
        <IosGroup
          header="Configuration summary"
          footer={
            showTransferStep && !state.transferData ? (
              <Text span inherit c="orange">
                Your existing providers and credentials will not be transferred.
              </Text>
            ) : undefined
          }
        >
          <IosRow title="Database" value={STORAGE_BACKEND_LABELS[state.backend]} />
          {state.backend === 'postgres' && (
            <IosRow title="Connection" value={summarizeConnection(state.postgresUrl) ?? 'Unavailable'} />
          )}
          <IosRow title="Credentials" value={secureStoreLabel()} />
          {showTransferStep && (
            <IosRow title="Data migration" value={state.transferData ? 'Transfer data' : 'Fresh start'} />
          )}
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
      <Card p="md">
        <Stack gap="sm">
          <Text size="sm" fw={600}>
            Configuration summary
          </Text>

          <Stack gap="xs">
            <Group justify="space-between" wrap="nowrap" gap="xs">
              <Text size="sm" c="dimmed" miw={LABEL_WIDTH} style={{ flexShrink: 0 }}>
                Database
              </Text>
              <Text size="sm" fw={500}>
                {STORAGE_BACKEND_LABELS[state.backend]}
              </Text>
            </Group>

            {state.backend === 'postgres' && (
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" miw={LABEL_WIDTH} style={{ flexShrink: 0 }}>
                  Connection
                </Text>
                <Text size="xs" {...(summarizeConnection(state.postgresUrl) ? { ff: 'monospace' } : {})} truncate>
                  {summarizeConnection(state.postgresUrl) ?? 'Unavailable'}
                </Text>
              </Group>
            )}

            {state.backend === 'sqlite' && !onDevice && (
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" miw={LABEL_WIDTH} style={{ flexShrink: 0 }}>
                  Data directory
                </Text>
                <Group gap="3xs" wrap="nowrap" miw={0}>
                  <Tooltip label={state.dataDir} multiline maw={300}>
                    <Text size="xs" {...(state.dataDir ? { ff: 'monospace' } : {})} truncate maw={isMobile ? 140 : 180}>
                      {truncatePath(state.dataDir) || 'Default'}
                    </Text>
                  </Tooltip>
                  <CopyButton value={state.dataDir} label="Copy data directory path" />
                </Group>
              </Group>
            )}

            <Group justify="space-between" wrap="nowrap" gap="xs">
              <Text size="sm" c="dimmed" miw={LABEL_WIDTH} style={{ flexShrink: 0 }}>
                Credentials
              </Text>
              <Text size="sm" fw={500}>
                {onDevice ? secureStoreLabel() : 'Encrypted storage'}
              </Text>
            </Group>

            {!vaultPasswordFromEnv && !onDevice && (
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" miw={LABEL_WIDTH} style={{ flexShrink: 0 }}>
                  Vault password
                </Text>
                <Group gap="3xs" wrap="nowrap">
                  <IconCheck size={14} color="var(--mantine-color-green-light-color)" aria-hidden />
                  <Text size="sm" c="green">
                    Set
                  </Text>
                </Group>
              </Group>
            )}

            {showTransferStep && (
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" miw={LABEL_WIDTH} style={{ flexShrink: 0 }}>
                  Data migration
                </Text>
                <Text size="sm" fw={500}>
                  {state.transferData ? 'Transfer data' : 'Fresh start'}
                </Text>
              </Group>
            )}
          </Stack>
        </Stack>
      </Card>

      {showEnvVarSection && (
        <Card p="md">
          <Stack gap="sm">
            <Text size="sm" fw={600}>
              Environment variable
            </Text>
            <Text size="xs" c="dimmed">
              Required to auto-unlock on next launch. Add to your shell profile.
            </Text>

            <Group gap="xs" wrap="nowrap" align="center">
              {exportCommand ? (
                <>
                  <Code block fz="xs" p="xs" style={{ flex: 1, wordBreak: 'break-all' }}>
                    {exportCommand}
                  </Code>
                  <CopyButton value={exportCommand} label="Copy export command" />
                </>
              ) : (
                <Text size="xs" c="red">
                  The variable name {envVarName} is not a valid shell variable name.
                </Text>
              )}
            </Group>
          </Stack>
        </Card>
      )}

      {showTransferStep && !state.transferData && (
        <Alert icon={<IconAlertCircle size={16} aria-hidden />} color="orange" variant="light">
          <Text size="xs">Your existing providers and credentials will not be transferred.</Text>
        </Alert>
      )}

      {error && (
        <Alert icon={<IconAlertCircle size={16} aria-hidden />} color="red" title="Couldn't save storage settings">
          {error}
        </Alert>
      )}
    </Stack>
  )
}
