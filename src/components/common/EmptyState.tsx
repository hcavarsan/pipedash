import { Group, Stack, Text, ThemeIcon } from '@mantine/core'
import type { ReactNode } from 'react'

interface EmptyStateProps {
  title: string
  description: string
  icon?: ReactNode
  action?: ReactNode
}

export function EmptyState({ title, description, icon, action }: EmptyStateProps) {
  return (
    <Stack align="center" gap="xs" py="xl" px="md" ta="center">
      {icon && (
        <ThemeIcon variant="light" color="gray" size={48} radius="xl" aria-hidden>
          {icon}
        </ThemeIcon>
      )}
      <Text fw={600}>{title}</Text>
      <Text size="sm" c="dimmed" maw={420}>
        {description}
      </Text>
      {action && (
        <Group gap="sm" justify="center" mt="xs">
          {action}
        </Group>
      )}
    </Stack>
  )
}
