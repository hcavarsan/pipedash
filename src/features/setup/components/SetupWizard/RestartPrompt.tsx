import { Button, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../../components/common/ErrorState'

interface RestartPromptProps {
  onDevice: boolean
  error: Error | null
  restarting: boolean
  onRestart: () => void
}

const COPY = {
  device: {
    noun: 'Reload',
    action: 'Reload now',
    failure: "Couldn't reload Pipedash",
    hint: 'Close and reopen Pipedash to load the new database.',
  },
  desktop: {
    noun: 'Restart',
    action: 'Restart now',
    failure: "Couldn't restart Pipedash",
    hint: 'Close and reopen Pipedash manually to load the new database.',
  },
}

export function RestartPrompt({ onDevice, error, restarting, onRestart }: RestartPromptProps): ReactNode {
  const copy = onDevice ? COPY.device : COPY.desktop

  return (
    <>
      <Text size="sm" c="dimmed" ta="center">
        {copy.noun} required to load the new database
      </Text>
      {error === null ? (
        <Button onClick={onRestart} loading={restarting}>
          {copy.action}
        </Button>
      ) : (
        <ErrorState title={copy.failure} error={error} hint={copy.hint} onRetry={onRestart} retrying={restarting} />
      )}
    </>
  )
}
