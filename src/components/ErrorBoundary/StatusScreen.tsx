import { Center, Stack, Text, Title } from '@mantine/core'
import type { ReactNode } from 'react'

interface StatusScreenProps {
  icon: ReactNode
  title: string
  message: ReactNode
  hint?: ReactNode
  detail?: ReactNode
  actions: ReactNode
  minHeight?: string | number
  headingOrder?: 1 | 2 | 3
}

export function StatusScreen({
  icon,
  title,
  message,
  hint,
  detail,
  actions,
  minHeight = '100%',
  headingOrder = 2,
}: StatusScreenProps): ReactNode {
  return (
    <Center mih={minHeight} h="100%" px="lg" py="xl" role="alert">
      <Stack align="center" gap="lg" w="100%" maw={360}>
        {icon}
        <Stack align="center" gap="2xs">
          <Title order={headingOrder} ta="center">
            {title}
          </Title>
          <Text size="sm" c="dimmed" ta="center" style={{ overflowWrap: 'anywhere' }}>
            {message}
          </Text>
          {hint && (
            <Text size="xs" c="dimmed" ta="center" style={{ overflowWrap: 'anywhere' }}>
              {hint}
            </Text>
          )}
        </Stack>
        {detail}
        <Stack gap="sm" w="100%">
          {actions}
        </Stack>
      </Stack>
    </Center>
  )
}
