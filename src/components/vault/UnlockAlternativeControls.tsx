import { Button } from '@mantine/core'
import { IconFaceId, IconFingerprint } from '@tabler/icons-react'
import { type ReactNode, useCallback, useEffect, useRef } from 'react'

import { useUnlockVault, useUnlockWithBiometrics } from '../../queries/useVaultQueries'
import type { AppLockStatus } from '../../types'
import { biometryLabel } from '../../utils/appLock'
import { errorMessage } from '../../utils/errorDisplay'

interface UnlockAlternativeControlsProps {
  biometricEnabled: boolean
  biometry: AppLockStatus['biometry']
  plainUnlock: boolean
  requiresPassword: boolean
  mobileShell: boolean
  disabled: boolean
  onSuccess: () => void
  onError: (message: string | null) => void
}

const UNLOCK_FAILED = "Couldn't unlock the vault. Check the password and try again."

export function UnlockAlternativeControls({
  biometricEnabled,
  biometry,
  plainUnlock,
  requiresPassword,
  mobileShell,
  disabled,
  onSuccess,
  onError,
}: UnlockAlternativeControlsProps): ReactNode {
  const autoPrompted = useRef(false)
  const { mutateAsync: unlockWithBiometrics, isPending: biometricPending } = useUnlockWithBiometrics()
  const { mutateAsync: unlockPlain, isPending: plainPending } = useUnlockVault()

  const handleBiometricUnlock = useCallback(async (): Promise<void> => {
    onError(null)

    try {
      const result = await unlockWithBiometrics()

      if (result.success) {
        onSuccess()
      } else if (result.reason !== 'cancelled') {
        onError(result.message || UNLOCK_FAILED)
      }
    } catch (err) {
      onError(errorMessage(err, UNLOCK_FAILED))
    }
  }, [unlockWithBiometrics, onSuccess, onError])

  useEffect(() => {
    if (biometricEnabled && !autoPrompted.current) {
      autoPrompted.current = true
      void handleBiometricUnlock()
    }
  }, [biometricEnabled, handleBiometricUnlock])

  const handlePlainUnlock = async (): Promise<void> => {
    onError(null)

    try {
      const result = await unlockPlain('')

      if (result.success) {
        onSuccess()
      } else {
        onError(result.message || UNLOCK_FAILED)
      }
    } catch (err) {
      onError(errorMessage(err, UNLOCK_FAILED))
    }
  }

  const size = mobileShell ? 'lg' : 'md'
  const iconSize = mobileShell ? 22 : 18
  const Icon = biometry === 'touchId' ? IconFingerprint : IconFaceId
  const biometricVariant = requiresPassword ? (mobileShell ? 'subtle' : 'light') : 'filled'
  const biometricPrefix = mobileShell && requiresPassword ? 'Use' : 'Unlock with'

  return (
    <>
      {biometricEnabled && (
        <Button
          fullWidth
          size={size}
          variant={biometricVariant}
          leftSection={<Icon size={iconSize} aria-hidden />}
          onClick={() => void handleBiometricUnlock()}
          loading={biometricPending}
          disabled={disabled && !biometricPending}
        >
          {biometricPrefix} {biometryLabel(biometry)}
        </Button>
      )}

      {plainUnlock && (
        <Button
          fullWidth
          size={size}
          onClick={() => void handlePlainUnlock()}
          loading={plainPending}
          disabled={disabled && !plainPending}
        >
          Unlock
        </Button>
      )}
    </>
  )
}
