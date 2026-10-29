import { Button, Code, Divider, PasswordInput, Stack, Text } from '@mantine/core'
import { IconFaceId, IconFingerprint } from '@tabler/icons-react'
import {
  type ChangeEvent,
  type KeyboardEvent,
  type ReactNode,
  type SyntheticEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useUnlockVault, useUnlockWithBiometrics, useVaultStatus } from '../../queries/useVaultQueries'
import { biometryLabel } from '../../utils/appLock'
import { errorMessage } from '../../utils/errorDisplay'
import { ErrorState } from '../common/ErrorState'
import { FormErrorDisplay } from '../common/FormErrorDisplay'
import { LoadingState } from '../common/LoadingState'

interface UnlockFormProps {
  onSuccess: () => void
  onCancel?: () => void
  showCancelButton?: boolean
}

export function UnlockForm({ onSuccess, onCancel, showCancelButton = false }: UnlockFormProps): ReactNode {
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [blurred, setBlurred] = useState({ password: false, confirmation: false })
  const autoPrompted = useRef(false)
  const confirmRef = useRef<HTMLInputElement>(null)

  const {
    data: vaultStatus,
    isLoading: isLoadingStatus,
    isError: statusFailed,
    error: statusError,
    refetch,
    isRefetching,
  } = useVaultStatus()
  const unlockMutation = useUnlockVault()
  const { mutateAsync: unlockWithBiometrics, isPending: biometricPending } = useUnlockWithBiometrics()
  const mobileShell = useMobileShell()

  const isFirstTime = mobileShell ? (vaultStatus?.is_first_time ?? true) : vaultStatus?.is_first_time === true
  const requiresPassword = vaultStatus?.requires_password ?? true
  const appLock = vaultStatus?.app_lock
  const biometricEnabled = appLock?.enabled === true
  const isLoading = unlockMutation.isPending || isLoadingStatus || biometricPending

  useEffect(() => {
    setPassword('')
    setConfirmPassword('')
    setError(null)
  }, [])

  const handleBiometricUnlock = useCallback(async () => {
    setError(null)

    try {
      const result = await unlockWithBiometrics()

      if (result.success) {
        onSuccess()
      } else if (result.reason !== 'cancelled') {
        setError(result.message || "Couldn't unlock the vault. Check the password and try again.")
      }
    } catch (err) {
      setError(errorMessage(err, "Couldn't unlock the vault. Check the password and try again."))
    }
  }, [unlockWithBiometrics, onSuccess])

  useEffect(() => {
    if (biometricEnabled && !autoPrompted.current) {
      autoPrompted.current = true
      void handleBiometricUnlock()
    }
  }, [biometricEnabled, handleBiometricUnlock])

  const handleUnlock = async () => {
    setError(null)
    if (!mobileShell) {
      setBlurred({ password: true, confirmation: true })
    }

    if (!password.trim()) {
      if (mobileShell) {
        setError('Password is required')
      }

      return
    }

    if (isFirstTime && password !== confirmPassword) {
      if (mobileShell) {
        setError('Passwords do not match')
      }

      return
    }

    try {
      const result = await unlockMutation.mutateAsync(password)

      if (result.success) {
        setPassword('')
        setConfirmPassword('')
        onSuccess()
      } else {
        setError(result.message || "Couldn't unlock the vault. Check the password and try again.")
      }
    } catch (err) {
      setError(errorMessage(err, "Couldn't unlock the vault. Check the password and try again."))
    }
  }

  const handleSubmit = (event: SyntheticEvent) => {
    event.preventDefault()

    if (!isLoading) {
      void handleUnlock()
    }
  }

  const focusConfirm = (event: KeyboardEvent<HTMLInputElement>) => {
    if (isFirstTime && event.key === 'Enter' && !event.nativeEvent.isComposing) {
      event.preventDefault()
      confirmRef.current?.focus()
    }
  }

  const passwordFieldProps = {
    value: password,
    onChange: (e: ChangeEvent<HTMLInputElement>) => {
      setPassword(e.currentTarget.value)
      if (!mobileShell) {
        setError(null)
      }
    },
    onBlur: () => setBlurred((value) => ({ ...value, password: true })),
    onKeyDown: focusConfirm,
    disabled: isLoading,
    error: mobileShell
      ? error && !confirmPassword
        ? error
        : undefined
      : blurred.password && !password.trim()
        ? 'Password is required'
        : undefined,
    size: 'md',
    autoComplete: isFirstTime ? 'new-password' : 'current-password',
    enterKeyHint: isFirstTime ? 'next' : 'go',
    autoFocus: !biometricEnabled,
    'data-autofocus': true,
  } as const

  const confirmFieldProps = {
    ref: confirmRef,
    value: confirmPassword,
    onChange: (e: ChangeEvent<HTMLInputElement>) => {
      setConfirmPassword(e.currentTarget.value)
      if (!mobileShell) {
        setError(null)
      }
    },
    onBlur: () => setBlurred((value) => ({ ...value, confirmation: true })),
    disabled: isLoading,
    error: mobileShell
      ? error && confirmPassword
        ? error
        : undefined
      : blurred.confirmation && password !== confirmPassword
        ? 'Passwords do not match'
        : undefined,
    size: 'md',
    autoComplete: 'new-password',
    enterKeyHint: 'go',
  } as const

  const plainUnlock = !requiresPassword && !biometricEnabled && appLock?.locked === true
  const demoHint = vaultStatus?.demo_password ? (
    <Text size="sm" c="dimmed" ta="center">
      This is a demo. The password is <Code>{vaultStatus.demo_password}</Code>
    </Text>
  ) : null

  const handlePlainUnlock = async () => {
    setError(null)

    try {
      const result = await unlockMutation.mutateAsync('')

      if (result.success) {
        onSuccess()
      } else {
        setError(result.message || "Couldn't unlock the vault. Check the password and try again.")
      }
    } catch (err) {
      setError(errorMessage(err, "Couldn't unlock the vault. Check the password and try again."))
    }
  }

  if (!mobileShell && isLoadingStatus) {
    return <LoadingState message="Loading vault status…" />
  }

  if (!mobileShell && statusFailed) {
    return (
      <ErrorState
        title="Couldn't load vault status"
        error={statusError}
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

  if (mobileShell) {
    return (
      <Stack gap="md">
        {demoHint}
        {requiresPassword && (
          <form noValidate onSubmit={handleSubmit}>
            <Stack gap="md">
              <Stack gap="sm">
                <PasswordInput aria-label="Password" placeholder="Password" {...passwordFieldProps} />

                {isFirstTime && (
                  <PasswordInput aria-label="Confirm password" placeholder="Confirm password" {...confirmFieldProps} />
                )}
              </Stack>

              <Button type="submit" fullWidth size="lg" loading={unlockMutation.isPending || isLoadingStatus}>
                Unlock
              </Button>
            </Stack>
          </form>
        )}

        {biometricEnabled && (
          <Button
            fullWidth
            size="lg"
            variant={requiresPassword ? 'subtle' : 'filled'}
            leftSection={appLock?.biometry === 'touchId' ? <IconFingerprint size={22} /> : <IconFaceId size={22} />}
            onClick={handleBiometricUnlock}
            loading={biometricPending}
            disabled={isLoading && !biometricPending}
          >
            {requiresPassword ? 'Use' : 'Unlock with'} {biometryLabel(appLock?.biometry)}
          </Button>
        )}

        {plainUnlock && (
          <Button fullWidth size="lg" onClick={handlePlainUnlock} loading={unlockMutation.isPending}>
            Unlock
          </Button>
        )}

        {!requiresPassword && error && (
          <Text size="sm" c="red" ta="center" role="alert">
            {error}
          </Text>
        )}

        {showCancelButton && onCancel && (
          <Button fullWidth variant="subtle" color="gray" size="lg" onClick={onCancel} disabled={isLoading}>
            Cancel
          </Button>
        )}
      </Stack>
    )
  }

  return (
    <Stack gap="lg">
      {demoHint}
      {biometricEnabled && (
        <Button
          fullWidth
          size="md"
          variant={requiresPassword ? 'light' : 'filled'}
          leftSection={appLock?.biometry === 'touchId' ? <IconFingerprint size={18} /> : <IconFaceId size={18} />}
          onClick={handleBiometricUnlock}
          loading={biometricPending}
          disabled={isLoading && !biometricPending}
        >
          Unlock with {biometryLabel(appLock?.biometry)}
        </Button>
      )}
      {plainUnlock && (
        <Button fullWidth size="md" onClick={handlePlainUnlock} loading={unlockMutation.isPending}>
          Unlock
        </Button>
      )}

      <FormErrorDisplay title="Couldn't unlock vault" globalError={error} onDismiss={() => setError(null)} />

      {requiresPassword && (
        <form noValidate onSubmit={handleSubmit}>
          <Stack gap="lg">
            {biometricEnabled && <Divider label="or use your password" />}

            <Stack gap="md">
              <PasswordInput label="Password" placeholder="Enter your password" {...passwordFieldProps} />

              {isFirstTime && (
                <PasswordInput label="Confirm password" placeholder="Re-enter your password" {...confirmFieldProps} />
              )}
            </Stack>

            <Button type="submit" fullWidth size="md" loading={unlockMutation.isPending || isLoadingStatus}>
              Unlock
            </Button>
          </Stack>
        </form>
      )}

      {showCancelButton && onCancel && (
        <Button fullWidth variant="subtle" size="md" onClick={onCancel} disabled={isLoading}>
          Cancel
        </Button>
      )}
    </Stack>
  )
}
