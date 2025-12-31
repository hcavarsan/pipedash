import { type ReactNode, useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { Center, Loader, Stack, Text } from '@mantine/core'

import { useVaultStatus } from '../../queries/useVaultQueries'
import { isTauri } from '../../services'
import { useAuthStore } from '../../stores/authStore'

interface RequireAuthProps {
  children: ReactNode
}

export function RequireAuth({ children }: RequireAuthProps) {
  const location = useLocation()
  const navigate = useNavigate()

  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
  const setReturnPath = useAuthStore((s) => s.setReturnPath)

  const { data: vaultStatus, isLoading: isLoadingVault } = useVaultStatus()

  const requiresPassword = vaultStatus?.requires_password ?? false
  const isVaultUnlocked = vaultStatus?.is_unlocked ?? false

  // Determine if we need to redirect to unlock
  // Desktop (Tauri): Only check vault status - keyring/IPC handles auth, no Bearer token needed
  // Web: Also require isAuthenticated because API requests need Bearer token in localStorage
  const needsUnlock = requiresPassword && (
    isTauri()
      ? !isVaultUnlocked
      : !isVaultUnlocked || !isAuthenticated
  )

  useEffect(() => {
    if (!isLoadingVault && needsUnlock) {
      // Save current location for redirect after unlock
      const currentPath = location.pathname + location.search


      setReturnPath(currentPath)
      navigate('/unlock', { replace: true })
    }
  }, [isLoadingVault, needsUnlock, location, setReturnPath, navigate])

  // Show loading while checking vault status
  if (isLoadingVault) {
    return (
      <Center h="100vh">
        <Stack align="center" gap="md">
          <Loader size="lg" />
          <Text size="sm" c="dimmed">
            Checking vault status...
          </Text>
        </Stack>
      </Center>
    )
  }

  // If needs unlock, show nothing (redirect in progress)
  if (needsUnlock) {
    return null
  }

  return <>{children}</>
}
