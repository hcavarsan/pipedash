import { Stack } from '@mantine/core'
import type { ReactNode } from 'react'

export const PageContainer = ({ children }: { children: ReactNode }) => (
  <Stack gap="md" w="100%" h="100%" mih={0}>
    {children}
  </Stack>
)
