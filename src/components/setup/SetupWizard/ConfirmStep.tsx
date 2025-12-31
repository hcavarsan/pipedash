import { ActionIcon, Alert, Card, Code, CopyButton, Group, Stack, Text, Tooltip } from '@mantine/core'
import { IconAlertCircle, IconCheck, IconCopy } from '@tabler/icons-react'

import type { ConfirmStepProps } from './types'

const BACKEND_LABELS: Record<string, string> = {
  sqlite: 'SQLite (Encrypted)',
  postgres: 'PostgreSQL',
}

const LABEL_WIDTH = 130

export function ConfirmStep({
  state,
  isDatabaseBackendChange: _isDatabaseBackendChange,
  isFromKeyring: _isFromKeyring,
  isDataDirChange: _isDataDirChange,
  showTransferStep,
  vaultPasswordFromEnv,
  needsVaultPassword,
  isMobile,
  error,
  getCurrentBackend: _getCurrentBackend,
  getCurrentDataDir: _getCurrentDataDir,
  vaultStatus,
}: ConfirmStepProps & { vaultStatus?: { env_var_name?: string } | null }) {
  const getBackendLabel = (type: string): string => BACKEND_LABELS[type] || type

  const truncatePath = (path: string): string => {
    const segments = path.split('/')

    if (segments.length <= 2) {
      return path
    }

    return `.../${segments.slice(-2).join('/')}`
  }

  const envVarName = vaultStatus?.env_var_name || 'PIPEDASH_VAULT_PASSWORD'
  const exportCommand = `export ${envVarName}="${state.vaultPassword}"`
  const showEnvVarSection = needsVaultPassword && !vaultPasswordFromEnv && state.vaultPassword

  return (
    <Stack gap="md">
      <Card p="md" withBorder>
        <Stack gap="sm">
          <Text size="sm" fw={600}>Configuration Summary</Text>

          <Stack gap="xs">
            <Group justify="space-between" wrap="nowrap" gap="xs">
              <Text size="sm" c="dimmed" style={{ minWidth: LABEL_WIDTH, flexShrink: 0 }}>
                Database
              </Text>
              <Text size="sm" fw={500}>
                {getBackendLabel(state.backend)}
              </Text>
            </Group>

            {state.backend === 'postgres' && (
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" style={{ minWidth: LABEL_WIDTH, flexShrink: 0 }}>
                  Connection
                </Text>
                <Text size="sm" c="dimmed" style={{ fontFamily: 'monospace', fontSize: '0.75rem' }} />
              </Group>
            )}

            {state.backend === 'sqlite' && (
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" style={{ minWidth: LABEL_WIDTH, flexShrink: 0 }}>
                  Data Directory
                </Text>
                <Group gap={4} wrap="nowrap" style={{ minWidth: 0 }}>
                  <Tooltip label={state.dataDir} withArrow multiline maw={300}>
                    <Text
                      size="xs"
                      style={{
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                        maxWidth: isMobile ? 140 : 180,
                        fontFamily: 'monospace',
                        cursor: 'help',
                      }}
                    >
                      {truncatePath(state.dataDir) || 'Default'}
                    </Text>
                  </Tooltip>
                  <CopyButton value={state.dataDir}>
                    {({ copied, copy }) => (
                      <Tooltip label={copied ? 'Copied!' : 'Copy path'} withArrow>
                        <ActionIcon
                          size="xs"
                          variant="subtle"
                          color={copied ? 'blue' : 'gray'}
                          onClick={copy}
                        >
                          {copied ? <IconCheck size={12} /> : <IconCopy size={12} />}
                        </ActionIcon>
                      </Tooltip>
                    )}
                  </CopyButton>
                </Group>
              </Group>
            )}

            <Group justify="space-between" wrap="nowrap" gap="xs">
              <Text size="sm" c="dimmed" style={{ minWidth: LABEL_WIDTH, flexShrink: 0 }}>
                Credentials
              </Text>
              <Text size="sm" fw={500}>
                Encrypted Storage
              </Text>
            </Group>

            {needsVaultPassword && !vaultPasswordFromEnv && (
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" style={{ minWidth: LABEL_WIDTH, flexShrink: 0 }}>
                  Vault Password
                </Text>
                <Group gap={4} wrap="nowrap">
                  <IconCheck size={14} style={{ color: 'var(--mantine-color-blue-5)' }} />
                  <Text size="sm" c="blue">Set</Text>
                </Group>
              </Group>
            )}

            {showTransferStep && (
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" style={{ minWidth: LABEL_WIDTH, flexShrink: 0 }}>
                  Data Migration
                </Text>
                <Text size="sm" fw={500}>
                  {state.transferData ? 'Transfer Data' : 'Fresh Start'}
                </Text>
              </Group>
            )}
          </Stack>
        </Stack>
      </Card>

      {showEnvVarSection && (
        <Card p="md" withBorder>
          <Stack gap="sm">
            <Text size="sm" fw={600}>Environment Variable</Text>
            <Text size="xs" c="dimmed">
              Required to auto-unlock on next launch. Add to your shell profile.
            </Text>

            <Group gap="xs" wrap="nowrap" align="stretch">
              <Code
                block
                style={{
                  flex: 1,
                  fontSize: '0.7rem',
                  wordBreak: 'break-all',
                  padding: '8px 12px',
                }}
              >
                {exportCommand}
              </Code>
              <CopyButton value={exportCommand}>
                {({ copied, copy }) => (
                  <Tooltip label={copied ? 'Copied!' : 'Copy'} withArrow>
                    <ActionIcon
                      size="lg"
                      variant="light"
                      color={copied ? 'blue' : 'gray'}
                      onClick={copy}
                      style={{ alignSelf: 'stretch', height: 'auto' }}
                    >
                      {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
                    </ActionIcon>
                  </Tooltip>
                )}
              </CopyButton>
            </Group>
          </Stack>
        </Card>
      )}

      {showTransferStep && !state.transferData && (
        <Alert icon={<IconAlertCircle size={16} />} color="yellow" variant="light">
          <Text size="xs">
            Your existing providers and credentials will not be transferred.
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
