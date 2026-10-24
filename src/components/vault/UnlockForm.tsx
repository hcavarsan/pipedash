import { Button, Divider, PasswordInput, Stack, Text } from '@mantine/core'
import { IconFaceId, IconFingerprint } from '@tabler/icons-react'
import { useCallback, useEffect, useRef, useState } from 'react'

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

  const { data: vaultStatus, isLoading: isLoadingStatus } = useVaultStatus()
  const unlockMutation = useUnlockVault()
  const { mutateAsync: unlockWithBiometrics, isPending: biometricPending } = useUnlockWithBiometrics()

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
      setError(err instanceof Error ? err.message : 'Failed to unlock vault')
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !isLoading) {
      handleUnlock()
    }
  }

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
        <Text size="sm" c="red" ta="center">
          {error}
        </Text>
      )}

      {requiresPassword && (
        <>
          {biometricEnabled && <Divider label="or use your password" />}

          <Stack gap="md">
            <PasswordInput
              label="Password"
              placeholder="Enter your password"
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
              onKeyDown={handleKeyDown}
              disabled={isLoading}
              error={error && !confirmPassword ? error : undefined}
              size="md"
              autoFocus={!biometricEnabled}
              data-autofocus
            />

            {isFirstTime && (
              <PasswordInput
                label="Confirm password"
                placeholder="Re-enter your password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.currentTarget.value)}
                onKeyDown={handleKeyDown}
                disabled={isLoading}
                error={error && confirmPassword ? error : undefined}
                size="md"
              />
            )}
          </Stack>

          <Button fullWidth size="md" onClick={handleUnlock} loading={unlockMutation.isPending || isLoadingStatus}>
            Unlock
          </Button>
        </>
      )}

      {showCancelButton && onCancel && (
        <Button fullWidth variant="subtle" size="md" onClick={onCancel} disabled={isLoading}>
          Cancel
        </Button>
      )}
    </Stack>
  )
}
