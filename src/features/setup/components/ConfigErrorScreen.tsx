import { Button, Code, ThemeIcon } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { StatusScreen } from '../../../components/ErrorBoundary/StatusScreen'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useStoragePaths } from '../../../queries/useStorageQueries'

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

  return (
    <StatusScreen
      icon={
        <ThemeIcon variant="transparent" color="red" size={48}>
          <IconAlertCircle size={24} aria-hidden />
        </ThemeIcon>
      }
      title="Couldn't load configuration"
      headingOrder={1}
      message={errors.join(', ') || 'Unknown error'}
      hint={hint}
      minHeight={mobileShell ? '100dvh' : '100%'}
      actions={
        <Button size={mobileShell ? 'md' : 'sm'} fullWidth loading={retrying} onClick={onRetry}>
          Retry
        </Button>
      }
    />
  )
}
