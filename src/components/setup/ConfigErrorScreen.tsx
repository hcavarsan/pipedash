import { Button, Center, Code, ThemeIcon } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useStoragePaths } from '../../queries/useStorageQueries'
import { ErrorState } from '../common/ErrorState'
import { StatusScreen } from '../ErrorBoundary/StatusScreen'

interface ConfigErrorScreenProps {
  errors: string[]
  onRetry: () => void
  retrying: boolean
}

export function ConfigErrorScreen({ errors, onRetry, retrying }: ConfigErrorScreenProps): ReactNode {
  const { data: storagePaths } = useStoragePaths()
  const mobileShell = useMobileShell()
  const hint = storagePaths ? (
    <>
      Fix the errors in <Code>{storagePaths.config_file}</Code>, then retry.
    </>
  ) : (
    'Fix the errors in your Pipedash configuration file, then retry.'
  )

  if (mobileShell) {
    return (
      <StatusScreen
        icon={
          <ThemeIcon variant="transparent" color="red" size={48}>
            <IconAlertCircle size={40} aria-hidden />
          </ThemeIcon>
        }
        title="Configuration file has errors"
        message={errors.join(', ') || 'Unknown error'}
        hint={hint}
        minHeight="100dvh"
        actions={
          <Button size="md" fullWidth loading={retrying} onClick={onRetry}>
            Retry
          </Button>
        }
      />
    )
  }

  return (
    <Center h="100%" p="xl">
      <Center maw={560} w="100%">
        <ErrorState
          title="Configuration file has errors"
          error={errors.join(', ') || 'Unknown error'}
          hint={hint}
          onRetry={onRetry}
          retrying={retrying}
        />
      </Center>
    </Center>
  )
}
