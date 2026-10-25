import { Group, Stack, Text, ThemeIcon } from '@mantine/core'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'

import { ContentUnavailable } from './ContentUnavailable'

interface EmptyStateProps {
  title: string
  description: string
  icon?: ReactNode
  action?: ReactNode
}

export function EmptyState({ title, description, icon, action }: EmptyStateProps) {
  const mobileShell = useMobileShell()

  if (mobileShell) {
    return <ContentUnavailable icon={icon} title={title} description={description} actions={action} />
  }

  return (
    <Stack align="center" gap="xs" py="xl" px="md" ta="center">
      {icon && (
        <ThemeIcon variant="transparent" color="gray" size={48} aria-hidden>
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
