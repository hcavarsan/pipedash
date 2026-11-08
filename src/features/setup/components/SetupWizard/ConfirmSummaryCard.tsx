import { Card, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { ConfirmSummaryRows } from './ConfirmSummaryRows'
import type { SetupState } from './types'

interface ConfirmSummaryCardProps {
  state: SetupState
  showTransferStep: boolean
  vaultPasswordFromEnv: boolean
  isMobile: boolean
}

export function ConfirmSummaryCard(props: ConfirmSummaryCardProps): ReactNode {
  return (
    <Card p="md">
      <Stack gap="sm">
        <Text size="sm" fw={600}>
          Configuration summary
        </Text>

        <ConfirmSummaryRows {...props} />
      </Stack>
    </Card>
  )
}
