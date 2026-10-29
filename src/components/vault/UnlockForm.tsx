import { Button, Code, Stack, Text } from '@mantine/core'
import { useIsMutating } from '@tanstack/react-query'
import { type ReactNode, useState } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useVaultStatus } from '../../queries/useVaultQueries'
import { ErrorState } from '../common/ErrorState'
import { FormErrorDisplay } from '../common/FormErrorDisplay'
import { LoadingState } from '../common/LoadingState'

import { UnlockAlternativeControls } from './UnlockAlternativeControls'
import { UnlockPasswordForm } from './UnlockPasswordForm'

type UnlockFormProps = { onSuccess: () => void } & (
  | { showCancelButton: true; onCancel: () => void }
  | { showCancelButton?: false; onCancel?: never }
)

export function UnlockForm({ onSuccess, onCancel, showCancelButton = false }: UnlockFormProps): ReactNode {
  const [alternativeError, setAlternativeError] = useState<string | null>(null)
  const { data: vaultStatus, isLoading: isLoadingStatus, isError, error, refetch, isRefetching } = useVaultStatus()
  const mobileShell = useMobileShell()
  const busy = useIsMutating() > 0 || isLoadingStatus

  const isFirstTime = mobileShell ? (vaultStatus?.is_first_time ?? true) : vaultStatus?.is_first_time === true
  const requiresPassword = vaultStatus?.requires_password ?? true
  const appLock = vaultStatus?.app_lock
  const biometricEnabled = appLock?.enabled === true
  const plainUnlock = !requiresPassword && !biometricEnabled && appLock?.locked === true

  if (!mobileShell && isLoadingStatus) {
    return <LoadingState message="Loading vault status…" />
  }

  if (!mobileShell && isError) {
    return (
      <ErrorState
        title="Couldn't load vault status"
        error={error}
        onRetry={() => {
          void refetch()
        }}
        retrying={isRefetching}
      />
    )
  }

  if (!requiresPassword && !biometricEnabled && !plainUnlock) {
    return null
  }

  const alternatives = (
    <UnlockAlternativeControls
      biometricEnabled={biometricEnabled}
      biometry={appLock?.biometry ?? 'none'}
      plainUnlock={plainUnlock}
      requiresPassword={requiresPassword}
      mobileShell={mobileShell}
      disabled={busy}
      onSuccess={onSuccess}
      onError={setAlternativeError}
    />
  )

  return (
    <Stack gap={mobileShell ? 'md' : 'lg'}>
      {vaultStatus?.demo_password && (
        <Text size="sm" c="dimmed" ta="center">
          This is a demo. The password is <Code>{vaultStatus.demo_password}</Code>
        </Text>
      )}
      {!mobileShell && alternatives}

      {mobileShell ? (
        alternativeError && (
          <Text size="sm" c="red" ta="center" role="alert">
            {alternativeError}
          </Text>
        )
      ) : (
        <FormErrorDisplay
          title="Couldn't unlock vault"
          globalError={alternativeError}
          onDismiss={() => setAlternativeError(null)}
        />
      )}

      {requiresPassword && (
        <UnlockPasswordForm
          firstTime={isFirstTime}
          mobileShell={mobileShell}
          disabled={busy}
          biometricEnabled={biometricEnabled}
          onSuccess={onSuccess}
        />
      )}
      {mobileShell && alternatives}

      {showCancelButton && onCancel && (
        <Button
          fullWidth
          variant="subtle"
          {...(mobileShell ? { color: 'gray' } : {})}
          size={mobileShell ? 'lg' : 'md'}
          onClick={onCancel}
          disabled={busy}
        >
          Cancel
        </Button>
      )}
    </Stack>
  )
}
