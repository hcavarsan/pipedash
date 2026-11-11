import { Group, Stack, Text } from '@mantine/core'
import { IconCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { STORAGE_BACKEND_LABELS } from '../../../../constants/storageLabels'
import { isMobilePlatform, secureStoreLabel } from '../../../../utils/platform'

import { summarizeConnection } from './confirmSummary'
import { DataDirectoryValue } from './DataDirectoryValue'
import { SummaryRow } from './SummaryRow'
import type { SetupState } from './types'

interface ConfirmSummaryRowsProps {
  state: SetupState
  showTransferStep: boolean
  vaultPasswordFromEnv: boolean
  isMobile: boolean
}

export function ConfirmSummaryRows({
  state,
  showTransferStep,
  vaultPasswordFromEnv,
  isMobile,
}: ConfirmSummaryRowsProps): ReactNode {
  const onDevice = isMobilePlatform()
  const connection = summarizeConnection(state.postgresUrl)

  return (
    <Stack gap="xs">
      <SummaryRow label="Database">
        <Text size="sm" fw={500}>
          {STORAGE_BACKEND_LABELS[state.backend]}
        </Text>
      </SummaryRow>

      {state.backend === 'postgres' && (
        <SummaryRow label="Connection">
          <Text size="xs" {...(connection ? { ff: 'monospace' } : {})} truncate>
            {connection ?? 'Unavailable'}
          </Text>
        </SummaryRow>
      )}

      {state.backend === 'sqlite' && !onDevice && (
        <SummaryRow label="Data directory">
          <DataDirectoryValue dataDir={state.dataDir} isMobile={isMobile} />
        </SummaryRow>
      )}

      <SummaryRow label="Credentials">
        <Text size="sm" fw={500}>
          {onDevice ? secureStoreLabel() : 'Encrypted storage'}
        </Text>
      </SummaryRow>

      {!vaultPasswordFromEnv && !onDevice && (
        <SummaryRow label="Vault password">
          <Group gap="3xs" wrap="nowrap">
            <IconCheck size={14} color="var(--mantine-color-green-light-color)" aria-hidden />
            <Text size="sm" c="green">
              Set
            </Text>
          </Group>
        </SummaryRow>
      )}

      {showTransferStep && (
        <SummaryRow label="Data migration">
          <Text size="sm" fw={500}>
            {state.transferData ? 'Transfer data' : 'Fresh start'}
          </Text>
        </SummaryRow>
      )}
    </Stack>
  )
}
