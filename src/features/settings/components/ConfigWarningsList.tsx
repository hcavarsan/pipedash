import { Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import type { ConfigAnalysisResponse } from '../../../types/storage'

interface ConfigWarningsListProps {
  warnings: ConfigAnalysisResponse['warnings']
}

export function ConfigWarningsList({ warnings }: ConfigWarningsListProps): ReactNode {
  return (
    <Stack gap="xs">
      {warnings.map((warning) => (
        <Text key={`${warning.field}:${warning.message}`} size="sm">
          • {warning.message}
        </Text>
      ))}
    </Stack>
  )
}
