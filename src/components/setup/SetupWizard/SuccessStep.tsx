import { Button, Card, Code, CopyButton, Group, SimpleGrid, Stack, Text, ThemeIcon, Tooltip } from '@mantine/core'
import { IconAlertTriangle, IconCheck, IconCopy } from '@tabler/icons-react'

import { isTauri, service } from '../../../services'

import type { SuccessStepProps } from './types'

const LABEL_WIDTH = 130

export function SuccessStep({
  state,
  migrationCompleted,
  transferResult,
  vaultStatus,
  vaultPasswordFromEnv,
  onComplete,
}: SuccessStepProps) {
  const envVarName = vaultStatus?.env_var_name || 'PIPEDASH_VAULT_PASSWORD'
  const exportCommand = `export ${envVarName}="${state.vaultPassword}"`

  const showPasswordReminder = !vaultPasswordFromEnv && state.vaultPassword

  return (
    <Stack gap="md" align="center" py="xl">
      <ThemeIcon size={56} radius="xl" variant="light" color="green">
        <IconCheck size={28} />
      </ThemeIcon>

      <Text size="lg" fw={600}>Setup Complete</Text>

      {migrationCompleted && transferResult?.success && (
        <Card p="md" withBorder w="100%">
          <Stack gap="sm">
            <Text size="sm" fw={600}>Migration Summary</Text>
            <SimpleGrid cols={3} spacing="xs">
              <Stack gap={2} align="center">
                <Text size="xs" c="dimmed">Providers</Text>
                <Text size="md" fw={600}>{transferResult.message.match(/(\d+) providers/)?.[1] || 0}</Text>
              </Stack>
              <Stack gap={2} align="center">
                <Text size="xs" c="dimmed">Tokens</Text>
                <Text size="md" fw={600}>{transferResult.message.match(/(\d+) tokens/)?.[1] || 0}</Text>
              </Stack>
              <Stack gap={2} align="center">
                <Text size="xs" c="dimmed">Cache</Text>
                <Text size="md" fw={600}>{transferResult.message.match(/(\d+) cache/)?.[1] || 0}</Text>
              </Stack>
            </SimpleGrid>
          </Stack>
        </Card>
      )}

      {showPasswordReminder && (
        <Card
          p="md"
          withBorder
          w="100%"
          style={{
            borderColor: 'var(--mantine-color-yellow-6)',
            backgroundColor: 'var(--mantine-color-yellow-light)',
          }}
        >
          <Stack gap="sm">
            <Group gap="xs">
              <IconAlertTriangle size={16} style={{ color: 'var(--mantine-color-yellow-6)' }} />
              <Text size="sm" fw={600}>Required for Next Launch</Text>
            </Group>

            <Stack gap="xs">
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" style={{ minWidth: LABEL_WIDTH, flexShrink: 0 }}>
                  Environment Variable
                </Text>
                <Text size="sm" fw={500}>{envVarName}</Text>
              </Group>

              <Group gap="xs" wrap="nowrap" align="stretch">
                <Code
                  block
                  style={{
                    flex: 1,
                    fontSize: '0.75rem',
                    wordBreak: 'break-all',
                    padding: '8px 12px',
                    backgroundColor: 'var(--mantine-color-dark-7)',
                  }}
                >
                  {exportCommand}
                </Code>
                <CopyButton value={exportCommand}>
                  {({ copied, copy }) => (
                    <Tooltip label={copied ? 'Copied!' : 'Copy'} withArrow>
                      <Button
                        size="sm"
                        variant="light"
                        color={copied ? 'green' : 'gray'}
                        onClick={copy}
                        style={{ alignSelf: 'stretch' }}
                      >
                        {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
                      </Button>
                    </Tooltip>
                  )}
                </CopyButton>
              </Group>

              <Text size="xs" c="dimmed">
                Add this to your shell profile (~/.zshrc or ~/.bashrc) or run before starting the app.
              </Text>
            </Stack>
          </Stack>
        </Card>
      )}

      {migrationCompleted && isTauri() ? (
        <>
          <Text size="sm" c="dimmed" ta="center">
            Restart required to load the new database
          </Text>
          <Button
            variant="light"
            color="blue"
            onClick={async () => {
              try {
                await service.restartApp()
              } catch (error) {
                console.error('Failed to restart:', error)
                onComplete()
              }
            }}
          >
            Restart Now
          </Button>
        </>
      ) : (
        <>
          <Text size="sm" c="dimmed" ta="center">
            Add providers to start monitoring pipelines
          </Text>
          <Button variant="light" color="blue" onClick={onComplete}>
            Get Started
          </Button>
        </>
      )}
    </Stack>
  )
}
