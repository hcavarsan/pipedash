import { Box, Center, Loader, Skeleton, Stack, Text } from '@mantine/core'

export interface LoadingStateProps {
  variant?: 'page' | 'section' | 'inline' | 'table'
  message?: string
  skeleton?: boolean
  skeletonLines?: number
  minHeight?: string | number
}

export function LoadingState({
  variant = 'page',
  message,
  skeleton = false,
  skeletonLines = 3,
  minHeight,
}: LoadingStateProps) {
  if (variant === 'page') {
    return (
      <Box style={{ minHeight: minHeight || '400px' }}>
        <Center py="xl" h="100%">
          <Stack align="center" gap="md">
            <Loader size="lg" />
            {message && (
              <Text size="sm" c="dimmed">
                {message}
              </Text>
            )}
          </Stack>
        </Center>
      </Box>
    )
  }

  if (variant === 'section') {
    return (
      <Box style={{ minHeight: minHeight || '200px' }}>
        <Center py="lg" h="100%">
          <Stack align="center" gap="sm">
            <Loader size="md" />
            {message && (
              <Text size="xs" c="dimmed">
                {message}
              </Text>
            )}
          </Stack>
        </Center>
      </Box>
    )
  }

  if (variant === 'table') {
    if (skeleton) {
      return (
        <Stack gap="xs" py="md">
          {Array.from({ length: skeletonLines }).map((_, i) => (
            <Skeleton key={i} height="2.5rem" radius="sm" />
          ))}
        </Stack>
      )
    }

    return (
      <Box style={{ minHeight: minHeight || '300px' }}>
        <Center py="xl" h="100%">
          <Loader size="lg" />
        </Center>
      </Box>
    )
  }

  return <Loader size="sm" />
}
