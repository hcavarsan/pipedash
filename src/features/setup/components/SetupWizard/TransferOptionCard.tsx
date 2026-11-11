import { Group, Radio, Stack, Text } from '@mantine/core'
import type { TablerIcon } from '@tabler/icons-react'
import type { ReactNode } from 'react'

interface TransferOptionCardProps {
  value: string
  Icon: TablerIcon
  title: string
  description: string
  recommended: boolean
}

export function TransferOptionCard({
  value,
  Icon,
  title,
  description,
  recommended,
}: TransferOptionCardProps): ReactNode {
  const badge = recommended ? (
    <Text size="xs" c="blue" fw={500}>
      Recommended
    </Text>
  ) : null

  return (
    <Radio.Card value={value} p="md" radius="md">
      <Group gap="md" wrap="nowrap" align="flex-start">
        <Radio.Indicator />
        <Stack gap="3xs" style={{ flex: 1 }}>
          <Group gap="xs" wrap="nowrap">
            <Icon size={16} aria-hidden />
            <Text size="sm" fw={500}>
              {title}
            </Text>
            {badge}
          </Group>
          <Text size="xs" c="dimmed">
            {description}
          </Text>
        </Stack>
      </Group>
    </Radio.Card>
  )
}
