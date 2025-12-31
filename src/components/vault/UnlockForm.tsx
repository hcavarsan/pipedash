import { useEffect, useState } from 'react'

import {
  Button,
  PasswordInput,
  Stack,
} from '@mantine/core'

import { useUnlockVault, useVaultStatus } from '../../queries/useVaultQueries'
import { useAuthStore } from '../../stores/authStore'

interface UnlockFormProps {
  onSuccess: () => void
  onCancel?: () => void
  showCancelButton?: boolean
}

export function UnlockForm({ onSuccess, onCancel, showCancelButton = false }: UnlockFormProps) {
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [error, setError] = useState<string | null>(null)

  const { data: vaultStatus, isLoading: isLoadingStatus } = useVaultStatus()
  const unlockMutation = useUnlockVault()
  const setToken = useAuthStore((s) => s.setToken)
  const clearToken = useAuthStore((s) => s.clearToken)

  const isFirstTime = vaultStatus?.is_first_time ?? true
  const requiresPassword = vaultStatus?.requires_password ?? true
  const isLoading = unlockMutation.isPending || isLoadingStatus

  useEffect(() => {
    setPassword('')
    setConfirmPassword('')
    setError(null)
  }, [])

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
      setToken(password)
      const result = await unlockMutation.mutateAsync(password)

      if (result.success) {
        setPassword('')
        setConfirmPassword('')
        onSuccess()
      } else {
        clearToken()
        setError(result.message || 'Failed to unlock vault')
      }
    } catch (err) {
      clearToken()
      setError(err instanceof Error ? err.message : 'Failed to unlock vault')
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !isLoading) {
      handleUnlock()
    }
  }

  if (!requiresPassword) {
    return null
  }

  return (
    <Stack gap="lg">
      <Stack gap="md">
        <PasswordInput
          placeholder="Enter your password"
          value={password}
          onChange={(e) => setPassword(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          disabled={isLoading}
          error={error && !confirmPassword ? error : undefined}
          size="md"
          autoFocus
          data-autofocus
        />

        {isFirstTime && (
          <PasswordInput
            label="Confirm Password"
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

      <Button
        fullWidth
        variant="light"
        color="blue"
        size="md"
        onClick={handleUnlock}
        loading={isLoading}
      >
        Unlock
      </Button>

      {showCancelButton && onCancel && (
        <Button
          fullWidth
          variant="subtle"
          size="md"
          onClick={onCancel}
          disabled={isLoading}
        >
          Cancel
        </Button>
      )}
    </Stack>
  )
}
