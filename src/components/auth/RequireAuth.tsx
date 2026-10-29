import { Button, Center, ThemeIcon } from '@mantine/core'
import { IconAlertCircle } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { Navigate, useLocation } from 'react-router-dom'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useVaultStatus } from '../../queries/useVaultQueries'
import { isTauri } from '../../services'
import { useAuthStore } from '../../stores/authStore'
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

  const isAuthenticated = useAuthStore((s) => s.token !== null)

  const { data: vaultStatus, isLoading: isLoadingVault, isError, error, refetch, isRefetching } = useVaultStatus()
  const mobileShell = useMobileShell()

  const needsUnlock = vaultStatus ? vaultNeedsUnlock(vaultStatus, isTauri(), isAuthenticated) : false

  if (isLoadingVault) {
    return <LoadingState variant="page" message="Checking vault status" />
  }

  if (isError && !vaultStatus) {
    if (mobileShell) {
      return (
        <StatusScreen
          headingOrder={1}
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

  if (needsUnlock) {
    return (
      <Navigate
        replace
        to="/unlock"
        state={{ returnTo: location.pathname + location.search, from: fromOf(location.state) }}
      />
    )
  }

  return <>{children}</>
}
