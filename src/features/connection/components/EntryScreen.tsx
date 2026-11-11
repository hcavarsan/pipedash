import { Box, Center, Paper, Stack } from '@mantine/core'
import type { ReactNode } from 'react'

import { DocumentTitle } from '../../../components/common/DocumentTitle'
import { ConnectionScreenHeading } from './ConnectionScreenHeading'

interface EntryScreenProps {
  title: string
  message?: string
  wrapTitle?: boolean
  documentTitle: string
  children: ReactNode
}

export function EntryScreen({ title, message, wrapTitle, documentTitle, children }: EntryScreenProps): ReactNode {
  return (
    <Box component="main" h="100dvh" className="safe-area">
      <DocumentTitle title={documentTitle} />
      <Center h="100%" p={{ base: 'md', sm: 'xl' }}>
        <Paper p={{ base: 'lg', sm: 'xl' }} w="100%" maw={480} withBorder>
          <Stack gap="xl">
            <ConnectionScreenHeading
              title={title}
              {...(message ? { message } : {})}
              {...(wrapTitle ? { wrapTitle } : {})}
            />

            {children}
          </Stack>
        </Paper>
      </Center>
    </Box>
  )
}
