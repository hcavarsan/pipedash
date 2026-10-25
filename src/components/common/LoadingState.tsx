import { Center, Loader, Stack, Text } from '@mantine/core'

import { useMobileShell } from '../../hooks/useIsMobile'

import { ContentUnavailable } from './ContentUnavailable'

interface LoadingStateProps {
  message: string
  variant?: 'page' | 'section'
}

export function LoadingState({ message, variant = 'section' }: LoadingStateProps) {
  const mobileShell = useMobileShell()
  const isPage = variant === 'page'

  if (mobileShell) {
    return <ContentUnavailable role="status" busy fill={isPage} icon={<Loader size="md" />} description={message} />
  }

  return (
    <Center h={isPage ? '100%' : undefined} mih={isPage ? 400 : 160} role="status">
      <Stack align="center" gap="sm">
        <Loader size={isPage ? 'lg' : 'md'} />
        <Text size="sm" c="dimmed">
          {message}
        </Text>
      </Stack>
    </Center>
  )
}
