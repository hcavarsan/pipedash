import { Center, Loader, Stack, Text } from '@mantine/core'

interface LoadingStateProps {
  message: string
  variant?: 'page' | 'section'
}

export function LoadingState({ message, variant = 'section' }: LoadingStateProps) {
  const isPage = variant === 'page'

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
