import { Alert, Card, Stack, Text } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import { STORAGE_BACKEND_LABELS } from '../../../../constants/storageLabels'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import { secureStoreLabel } from '../../../../utils/platform'

import { ConfirmSummaryCard } from './ConfirmSummaryCard'
import { summarizeConnection } from './confirmSummary'
import type { ConfirmStepProps } from './types'
import { VaultExportCommand } from './VaultExportCommand'

export function ConfirmStep({
  state,
  showTransferStep,
  vaultPasswordFromEnv,
  isMobile,
  error,
  vaultStatus,
}: ConfirmStepProps): ReactNode {
  const mobileShell = useMobileShell()

  const envVarName = vaultStatus.env_var_name || 'PIPEDASH_VAULT_PASSWORD'
  const showEnvVarSection = !vaultPasswordFromEnv && state.vaultPassword
  const skipsTransfer = showTransferStep && !state.transferData

  if (mobileShell) {
    const transferNotice = skipsTransfer ? (
      <Text span inherit c="orange">
        Your existing providers and credentials will not be transferred.
      </Text>
    ) : undefined

    return (
      <Stack gap="md">
        <IosGroup header="Configuration summary" footer={transferNotice}>
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
      <ConfirmSummaryCard
        state={state}
        showTransferStep={showTransferStep}
        vaultPasswordFromEnv={vaultPasswordFromEnv}
        isMobile={isMobile}
      />

      {showEnvVarSection && (
        <Card p="md">
          <Stack gap="sm">
            <Text size="sm" fw={600}>
              Environment variable
            </Text>
            <Text size="xs" c="dimmed">
              Required to auto-unlock on next launch. Add to your shell profile.
            </Text>

            <VaultExportCommand envVarName={envVarName} password={state.vaultPassword} />
          </Stack>
        </Card>
      )}

      {skipsTransfer && (
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
