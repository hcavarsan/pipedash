import { Center, Group, Loader, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'

import { ContentUnavailable } from './ContentUnavailable'

interface LoadingStateProps {
  message: string
  variant?: 'page' | 'section' | 'compact'
  action?: ReactNode
}

export function LoadingState({ message, variant = 'section', action }: LoadingStateProps) {
  const mobileShell = useMobileShell()
  const isPage = variant === 'page'

  if (mobileShell) {
    return <ContentUnavailable role="status" busy fill={isPage} icon={<Loader size="md" />} description={message} />
  }

  if (variant === 'compact') {
    return (
      <Stack gap="xs" role="status">
        <Group gap="xs" wrap="nowrap">
          <Loader size="xs" aria-hidden />
          <Text size="sm" c="dimmed">
            {message}
          </Text>
        </Group>
        {action}
      </Stack>
    )
  }

  return (
    <Center h={isPage ? '100%' : undefined} mih={isPage ? 400 : 160} role="status">
      <Stack align="center" gap="sm">
        <Loader size={isPage ? 'lg' : 'md'} />
        <Text size="sm" c="dimmed">
          {message}
        </Text>
        {action}
      </Stack>
    </Center>
  )
}
