import { Button, Center, ThemeIcon } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import { type ReactNode, useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useVaultStatus } from '../../queries/useVaultQueries'
import { isTauri } from '../../services'
import { useAuthStore } from '../../stores/authStore'
import { useMobileNavStore } from '../../stores/mobileNavStore'
import { formatErrorMessage } from '../../types/errors'
import { needsUnlock as vaultNeedsUnlock } from '../../utils/appLock'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StatusScreen } from '../ErrorBoundary/StatusScreen'
import { fromOf } from '../layout/mobileChrome'

interface RequireAuthProps {
  children: ReactNode
}

export function RequireAuth({ children }: RequireAuthProps): ReactNode {
  const location = useLocation()
  const navigate = useNavigate()

  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
  const setReturnPath = useAuthStore((s) => s.setReturnPath)

  const { data: vaultStatus, isLoading: isLoadingVault, isError, error, refetch, isRefetching } = useVaultStatus()
  const mobileShell = useMobileShell()
  const hold = useMobileNavStore((s) => s.hold)

  const needsUnlock = vaultStatus ? vaultNeedsUnlock(vaultStatus, isTauri(), isAuthenticated) : false

  useEffect(() => {
    if (!isLoadingVault && needsUnlock) {
      // Save current location for redirect after unlock
      const currentPath = location.pathname + location.search

      setReturnPath(currentPath)
      hold({ path: currentPath, from: fromOf(location.state) })
      void navigate('/unlock', { replace: true })
    }
  }, [isLoadingVault, needsUnlock, location, setReturnPath, hold, navigate])

  if (isLoadingVault) {
    return <LoadingState variant="page" message="Checking vault status" />
  }

  if (isError && !vaultStatus) {
    if (mobileShell) {
      return (
        <StatusScreen
          icon={
            <ThemeIcon variant="transparent" color="red" size={48}>
              <IconAlertCircle size={40} aria-hidden />
            </ThemeIcon>
          }
          title="Couldn't check vault status"
          message={formatErrorMessage(error)}
          actions={
            <Button size="md" fullWidth loading={isRefetching} onClick={() => refetch()}>
              Retry
            </Button>
          }
        />
      )
    }

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
