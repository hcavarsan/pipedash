import { Center } from '@mantine/core'
import { type ReactNode, useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { useVaultStatus } from '../../queries/useVaultQueries'
import { isTauri } from '../../services'
import { useAuthStore } from '../../stores/authStore'
import { needsUnlock as vaultNeedsUnlock } from '../../utils/appLock'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'

interface RequireAuthProps {
  children: ReactNode
}

export function RequireAuth({ children }: RequireAuthProps) {
  const location = useLocation()
  const navigate = useNavigate()

  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
  const setReturnPath = useAuthStore((s) => s.setReturnPath)

  const { data: vaultStatus, isLoading: isLoadingVault, isError, error, refetch, isRefetching } = useVaultStatus()

  const needsUnlock = vaultStatus ? vaultNeedsUnlock(vaultStatus, isTauri(), isAuthenticated) : false

  useEffect(() => {
    if (!isLoadingVault && needsUnlock) {
      // Save current location for redirect after unlock
      const currentPath = location.pathname + location.search

      setReturnPath(currentPath)
      navigate('/unlock', { replace: true })
    }
  }, [isLoadingVault, needsUnlock, location, setReturnPath, navigate])

  if (isLoadingVault) {
    return <LoadingState variant="page" message="Checking vault status" />
  }

  if (isError && !vaultStatus) {
    return (
      <Center h="100%" p="xl">
        <ErrorState
          title="Couldn't check vault status"
          error={error}
          onRetry={() => refetch()}
          retrying={isRefetching}
        />
      </Center>
    )
  }

  // If needs unlock, show nothing (redirect in progress)
  if (needsUnlock) {
    return null
  }

  return <>{children}</>
}
