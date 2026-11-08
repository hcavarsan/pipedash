import { Button, Stack, Text, ThemeIcon, Title } from '@mantine/core'
import { IconCheck, IconCircleCheckFilled } from '@tabler/icons-react'
import { useMutation } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../../components/common/ErrorState'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import { logger } from '../../../../lib/logger'
import { hasLocalCore } from '../../../../services/connection'
import { service } from '../../../../services/service'
import { isMobilePlatform } from '../../../../utils/platform'

import { MigrationSummaryCard } from './MigrationSummaryCard'
import { PasswordReminderCard } from './PasswordReminderCard'
import { RestartPrompt } from './RestartPrompt'
import type { SuccessStepProps } from './types'

export function SuccessStep({
  state,
  migrationStats,
  vaultStatus,
  vaultPasswordFromEnv,
  onComplete,
}: SuccessStepProps): ReactNode {
  const onDevice = isMobilePlatform()
  const mobileShell = useMobileShell()
  const restart = useMutation({
    mutationFn: () => service.restartApp(),
    onError: (error) => logger.error('SuccessStep', 'Failed to restart app', error),
  })
  const restarting = restart.isPending || restart.isSuccess
  const restartError = restart.error

  const envVarName = vaultStatus.env_var_name || 'PIPEDASH_VAULT_PASSWORD'

  const showPasswordReminder = !onDevice && !vaultPasswordFromEnv && state.vaultPassword

  const handleRestart = (): void => restart.mutate()

  const migratedCounts = migrationStats
    ? [
        { label: 'Providers', value: migrationStats.providers_migrated },
        { label: 'Tokens', value: migrationStats.tokens_migrated },
        { label: 'Cache entries', value: migrationStats.cache_entries_migrated },
        { label: 'Metrics', value: migrationStats.metrics_migrated },
      ]
    : []

  if (mobileShell) {
    const needsReload = migrationStats !== null && hasLocalCore()

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
        <IconCheck size={28} aria-hidden />
      </ThemeIcon>

      <Text size="lg" fw={600}>
        Setup complete
      </Text>

      {migrationStats !== null && <MigrationSummaryCard counts={migratedCounts} />}

      {showPasswordReminder && <PasswordReminderCard envVarName={envVarName} password={state.vaultPassword} />}

      {migrationStats !== null && hasLocalCore() ? (
        <RestartPrompt onDevice={onDevice} error={restartError} restarting={restarting} onRestart={handleRestart} />
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
