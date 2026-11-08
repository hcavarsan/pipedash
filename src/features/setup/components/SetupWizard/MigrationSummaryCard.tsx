import { Card, SimpleGrid, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

interface MigrationCount {
  label: string
  value: number
}

function MigrationCountCell({ label, value }: MigrationCount): ReactNode {
  return (
    <Stack gap="4xs" align="center">
      <Text size="xs" c="dimmed">
        {label}
      </Text>
      <Text size="md" fw={600}>
        {value}
      </Text>
    </Stack>
  )
}

export function MigrationSummaryCard({ counts }: { counts: MigrationCount[] }): ReactNode {
  return (
    <Card p="md" w="100%">
      <Stack gap="sm">
        <Text size="sm" fw={600}>
          Migration summary
        </Text>
        <SimpleGrid cols={4} spacing="xs">
          {counts.map(({ label, value }) => (
            <MigrationCountCell key={label} label={label} value={value} />
          ))}
        </SimpleGrid>
      </Stack>
    </Card>
  )
}
