import { Stack, Text, Title } from '@mantine/core'
import type { ReactNode } from 'react'

import { AppLogo } from '../../../components/atoms/AppLogo'

interface ConnectionScreenHeadingProps {
  title: string
  message?: string
  wrapTitle?: boolean
}

export function ConnectionScreenHeading({ title, message, wrapTitle }: ConnectionScreenHeadingProps): ReactNode {
  return (
    <Stack gap="md" align="center">
      <AppLogo size={{ base: 48, sm: 56 }} alt="Pipedash" />
      <Title order={1} size="h2" ta="center" {...(wrapTitle ? { style: { overflowWrap: 'anywhere' } } : {})}>
        {title}
      </Title>
      {message && (
        <Text c="dimmed" ta="center" style={{ overflowWrap: 'anywhere' }}>
          {message}
        </Text>
      )}
    </Stack>
  )
}
