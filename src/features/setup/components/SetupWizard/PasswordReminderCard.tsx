import { Card, Group, Stack, Text } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { SummaryRow } from './SummaryRow'
import { VaultExportCommand } from './VaultExportCommand'

interface PasswordReminderCardProps {
  envVarName: string
  password: string
}

export function PasswordReminderCard({ envVarName, password }: PasswordReminderCardProps): ReactNode {
  return (
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
          <IconAlertTriangle size={16} color="var(--mantine-color-orange-light-color)" aria-hidden />
          <Text size="sm" fw={600}>
            Required for next launch
          </Text>
        </Group>

        <Stack gap="xs">
          <SummaryRow label="Environment variable">
            <Text size="sm" fw={500}>
              {envVarName}
            </Text>
          </SummaryRow>

          <VaultExportCommand envVarName={envVarName} password={password} />

          <Text size="xs" c="dimmed">
            Add this to your shell profile (~/.zshrc or ~/.bashrc) or run before starting the app.
          </Text>
        </Stack>
      </Stack>
    </Card>
  )
}
