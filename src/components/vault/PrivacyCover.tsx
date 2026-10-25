import { Center } from '@mantine/core'
import { useEffect, useSyncExternalStore } from 'react'

import { useVaultStatus } from '../../queries/useVaultQueries'
import { mobileService } from '../../services/mobile'
import { shouldShowPrivacyCover } from '../../utils/appLock'
import { isMobilePlatform } from '../../utils/platform'

const subscribe = (onChange: () => void) => {
  document.addEventListener('visibilitychange', onChange)

  return () => document.removeEventListener('visibilitychange', onChange)
}

const getVisibility = () => document.visibilityState

export function PrivacyCover() {
  const visibility = useSyncExternalStore(subscribe, getVisibility)
  const { data: vaultStatus, refetch } = useVaultStatus({ enabled: isMobilePlatform() })
  const appLock = vaultStatus?.app_lock

  useEffect(() => {
    if (visibility === 'visible' && isMobilePlatform()) {
      refetch()
        .finally(() => mobileService.releasePrivacyCover())
        .catch(() => undefined)
    }
  }, [visibility, refetch])

  if (!shouldShowPrivacyCover(appLock?.enabled === true, visibility)) {
    return null
  }

  return (
    <Center
      aria-hidden
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 10000,
        background: 'var(--ios-grouped-bg, var(--mantine-color-body))',
      }}
    />
  )
}
