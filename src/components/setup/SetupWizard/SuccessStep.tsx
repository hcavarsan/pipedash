import { Button, Card, Code, Group, SimpleGrid, Stack, Text, ThemeIcon, Title } from '@mantine/core'
import { IconAlertTriangle, IconCheck, IconCircleCheckFilled } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'

import { useMobileShell } from '../../../hooks/useIsMobile'
import { logger } from '../../../lib/logger'
import { isTauri, service } from '../../../services'
import { isMobilePlatform } from '../../../utils/platform'
import { CopyButton } from '../../atoms/CopyButton'
import { ErrorState } from '../../common/ErrorState'
import { IosGroup, IosRow } from '../../ios/IosList'

import type { SuccessStepProps } from './types'

const LABEL_WIDTH = 130

export function SuccessStep({
  state,
  migrationStats,
  vaultStatus,
  vaultPasswordFromEnv,
  onComplete,
}: SuccessStepProps): ReactNode {
  const [restarting, setRestarting] = useState(false)
  const onDevice = isMobilePlatform()
  const mobileShell = useMobileShell()
  const [restartError, setRestartError] = useState<unknown>(null)

  const envVarName = vaultStatus?.env_var_name || 'PIPEDASH_VAULT_PASSWORD'
  const exportCommand = `export ${envVarName}="${state.vaultPassword}"`

  const showPasswordReminder = !onDevice && !vaultPasswordFromEnv && state.vaultPassword

  const handleRestart = async () => {
    setRestarting(true)
    setRestartError(null)

    try {
      await service.restartApp()
    } catch (error) {
      logger.error('SuccessStep', 'Failed to restart app', error)
      setRestartError(error)
      setRestarting(false)
    }
  }

  const migratedCounts = migrationStats
    ? [
        { label: 'Providers', value: migrationStats.providers_migrated },
        { label: 'Tokens', value: migrationStats.tokens_migrated },
        { label: 'Cache entries', value: migrationStats.cache_entries_migrated },
        { label: 'Metrics', value: migrationStats.metrics_migrated },
      ]
    : []

  if (mobileShell) {
    const needsReload = migrationStats !== null && isTauri()

    return (
      <Stack gap="xl" style={{ flex: 1 }}>
        <Stack align="center" gap="xs" pt="xl">
          <IconCircleCheckFilled size={80} color="var(--ios-green)" aria-hidden />
          <Title order={2} ta="center" tabIndex={-1} data-step-heading>
            Setup complete
          </Title>
          <Text size="sm" c="dimmed" ta="center">
            {needsReload ? 'Reload required to load the new database' : 'Add providers to start monitoring pipelines'}
          </Text>
        </Stack>

        {migrationStats !== null && (
          <IosGroup header="Migration summary">
            {migratedCounts.map(({ label, value }) => (
              <IosRow key={label} title={label} value={value} />
            ))}
          </IosGroup>
        )}

        {needsReload && restartError !== null && (
          <ErrorState
            title="Couldn't reload Pipedash"
            error={restartError}
            hint="Close and reopen Pipedash to load the new database."
            onRetry={handleRestart}
            retrying={restarting}
          />
        )}

        {(!needsReload || restartError === null) && (
          <Button
            size="lg"
            fullWidth
            mt="auto"
            onClick={needsReload ? handleRestart : onComplete}
            loading={needsReload && restarting}
          >
            {needsReload ? 'Reload now' : 'Get started'}
          </Button>
        )}
      </Stack>
    )
  }

  return (
    <Stack gap="md" align="center" py="xl">
      <ThemeIcon size={56} variant="transparent" color="green">
        <IconCheck size={28} />
      </ThemeIcon>

      <Text size="lg" fw={600}>
        Setup complete
      </Text>

      {migrationStats !== null && (
        <Card p="md" w="100%">
          <Stack gap="sm">
            <Text size="sm" fw={600}>
              Migration summary
            </Text>
            <SimpleGrid cols={4} spacing="xs">
              {migratedCounts.map(({ label, value }) => (
                <Stack key={label} gap="4xs" align="center">
                  <Text size="xs" c="dimmed">
                    {label}
                  </Text>
                  <Text size="md" fw={600}>
                    {value}
                  </Text>
                </Stack>
              ))}
            </SimpleGrid>
          </Stack>
        </Card>
      )}

      {showPasswordReminder && (
        <Card
          p="md"
          w="100%"
          style={{
            borderColor: 'var(--mantine-color-orange-light-color)',
            backgroundColor: 'var(--mantine-color-orange-light)',
          }}
        >
          <Stack gap="sm">
            <Group gap="xs">
              <IconAlertTriangle size={16} color="var(--mantine-color-orange-light-color)" />
              <Text size="sm" fw={600}>
                Required for next launch
              </Text>
            </Group>

            <Stack gap="xs">
              <Group justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" c="dimmed" miw={LABEL_WIDTH} style={{ flexShrink: 0 }}>
                  Environment variable
                </Text>
                <Text size="sm" fw={500}>
                  {envVarName}
                </Text>
              </Group>

              <Group gap="xs" wrap="nowrap" align="center">
                <Code block fz="xs" p="xs" style={{ flex: 1, wordBreak: 'break-all' }}>
                  {exportCommand}
                </Code>
                <CopyButton value={exportCommand} label="Copy export command" />
              </Group>

              <Text size="xs" c="dimmed">
                Add this to your shell profile (~/.zshrc or ~/.bashrc) or run before starting the app.
              </Text>
            </Stack>
          </Stack>
        </Card>
      )}

      {migrationStats !== null && isTauri() ? (
        <>
          <Text size="sm" c="dimmed" ta="center">
            {onDevice ? 'Reload' : 'Restart'} required to load the new database
          </Text>
          {restartError === null ? (
            <Button onClick={handleRestart} loading={restarting}>
              {onDevice ? 'Reload now' : 'Restart now'}
            </Button>
          ) : (
            <ErrorState
              title={onDevice ? "Couldn't reload Pipedash" : "Couldn't restart Pipedash"}
              error={restartError}
              hint={
                onDevice
                  ? 'Close and reopen Pipedash to load the new database.'
                  : 'Close and reopen Pipedash manually to load the new database.'
              }
              onRetry={handleRestart}
              retrying={restarting}
            />
          )}
        </>
      ) : (
        <>
          <Text size="sm" c="dimmed" ta="center">
            Add providers to start monitoring pipelines
          </Text>
          <Button onClick={onComplete}>Get started</Button>
        </>
      )}
    </Stack>
  )
}
