import { Button, Divider, PasswordInput, Stack, Text } from '@mantine/core'
import { IconFaceId, IconFingerprint } from '@tabler/icons-react'
import {
  type ChangeEvent,
  type KeyboardEvent,
  type SyntheticEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useUnlockVault, useUnlockWithBiometrics, useVaultStatus } from '../../queries/useVaultQueries'
import { formatErrorMessage } from '../../types/errors'
import { biometryLabel } from '../../utils/appLock'

interface UnlockFormProps {
  onSuccess: () => void
  onCancel?: () => void
  showCancelButton?: boolean
}

export function UnlockForm({ onSuccess, onCancel, showCancelButton = false }: UnlockFormProps) {
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const autoPrompted = useRef(false)
  const confirmRef = useRef<HTMLInputElement>(null)

  const { data: vaultStatus, isLoading: isLoadingStatus } = useVaultStatus()
  const unlockMutation = useUnlockVault()
  const { mutateAsync: unlockWithBiometrics, isPending: biometricPending } = useUnlockWithBiometrics()
  const mobileShell = useMobileShell()

  const isFirstTime = vaultStatus?.is_first_time ?? true
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
        setError(result.message || 'Failed to unlock')
      }
    } catch (err) {
      setError(formatErrorMessage(err))
    }
  }, [unlockWithBiometrics, onSuccess])

  useEffect(() => {
    if (biometricEnabled && !autoPrompted.current) {
      autoPrompted.current = true
      handleBiometricUnlock()
    }
  }, [biometricEnabled, handleBiometricUnlock])

  const handleUnlock = async () => {
    setError(null)

    if (!password.trim()) {
      setError('Password is required')

      return
    }

    if (isFirstTime && password !== confirmPassword) {
      setError('Passwords do not match')

      return
    }

    try {
      const result = await unlockMutation.mutateAsync(password)

      if (result.success) {
        setPassword('')
        setConfirmPassword('')
        onSuccess()
      } else {
        setError(result.message || 'Failed to unlock vault')
      }
    } catch (err) {
      setError(formatErrorMessage(err))
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
    onChange: (e: ChangeEvent<HTMLInputElement>) => setPassword(e.currentTarget.value),
    onKeyDown: focusConfirm,
    disabled: isLoading,
    error: error && !confirmPassword ? error : undefined,
    size: 'md',
    autoComplete: isFirstTime ? 'new-password' : 'current-password',
    enterKeyHint: isFirstTime ? 'next' : 'go',
    autoFocus: !biometricEnabled,
    'data-autofocus': true,
  } as const

  const confirmFieldProps = {
    ref: confirmRef,
    value: confirmPassword,
    onChange: (e: ChangeEvent<HTMLInputElement>) => setConfirmPassword(e.currentTarget.value),
    disabled: isLoading,
    error: error && confirmPassword ? error : undefined,
    size: 'md',
    autoComplete: 'new-password',
    enterKeyHint: 'go',
  } as const

  const plainUnlock = !requiresPassword && !biometricEnabled && appLock?.locked === true

  const handlePlainUnlock = async () => {
    setError(null)

    try {
      const result = await unlockMutation.mutateAsync('')

      if (result.success) {
        onSuccess()
      } else {
        setError(result.message || 'Failed to unlock')
      }
    } catch (err) {
      setError(formatErrorMessage(err))
    }
  }

  if (!requiresPassword && !biometricEnabled && !plainUnlock) {
    return null
  }

  if (mobileShell) {
    return (
      <Stack gap="md">
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

      {!requiresPassword && error && (
        <Text size="sm" c="red" ta="center" role="alert">
          {error}
        </Text>
      )}

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
