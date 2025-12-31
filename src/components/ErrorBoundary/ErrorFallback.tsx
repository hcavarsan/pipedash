import { Alert, Button, Card, Code, Collapse, Stack, Text } from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { IconAlertTriangle, IconRefresh } from '@tabler/icons-react'

import { formatErrorMessage } from '../../types/errors'

interface ErrorFallbackProps {
  error: Error
  resetError: () => void
  title?: string
  showDetails?: boolean
}

export function ErrorFallback({
  error,
  resetError,
  title = 'Something went wrong',
  showDetails = true,
}: ErrorFallbackProps) {
  const [detailsOpened, { toggle }] = useDisclosure(false)

  return (
    <Card withBorder padding="lg" radius="md" style={{ maxWidth: 600, margin: '0 auto' }}>
      <Stack gap="md">
        <Alert
          icon={<IconAlertTriangle size={20} />}
          title={title}
          color="red"
          variant="filled"
        >
          {formatErrorMessage(error)}
        </Alert>

        {showDetails && (
          <>
            <Button
              variant="subtle"
              size="xs"
              onClick={toggle}
            >
              {detailsOpened ? 'Hide' : 'Show'} technical details
            </Button>

            <Collapse in={detailsOpened}>
              <Stack gap="xs">
                <Text size="sm" fw={500}>Error details:</Text>
                <Code block style={{ whiteSpace: 'pre-wrap' }}>
                  {error.stack || error.message}
                </Code>
              </Stack>
            </Collapse>
          </>
        )}

        <Button
          leftSection={<IconRefresh size={16} />}
          onClick={resetError}
          variant="light"
          color="blue"
        >
          Try again
        </Button>
      </Stack>
    </Card>
  )
}
