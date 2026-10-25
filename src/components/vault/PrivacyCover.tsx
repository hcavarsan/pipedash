import { Center } from '@mantine/core'
import { useEffect, useState, useSyncExternalStore } from 'react'

import { useVaultStatus } from '../../queries/useVaultQueries'
import { mobileService } from '../../services/mobile'
import { nextCoverLatch, shouldShowPrivacyCover } from '../../utils/appLock'
import { isMobilePlatform } from '../../utils/platform'
import { AppLogo } from '../atoms/AppLogo'

const subscribe = (onChange: () => void) => {
  document.addEventListener('visibilitychange', onChange)

  return () => document.removeEventListener('visibilitychange', onChange)
}

const getVisibility = () => document.visibilityState

export function PrivacyCover() {
  const visibility = useSyncExternalStore(subscribe, getVisibility)
  const { data: vaultStatus, refetch } = useVaultStatus({ enabled: isMobilePlatform() })
  const [latched, setLatched] = useState(false)
  const appLock = vaultStatus?.app_lock
  const nextLatched = nextCoverLatch(latched, appLock, visibility)

  if (nextLatched !== latched) {
    setLatched(nextLatched)
  }

  useEffect(() => {
    if (visibility === 'visible' && isMobilePlatform()) {
      refetch()
        .finally(() => mobileService.releasePrivacyCover())
        .catch(() => undefined)
    }
  }, [visibility, refetch])

  if (!shouldShowPrivacyCover(appLock?.enabled === true, visibility, nextLatched)) {
    return null
  }

  return (
    <Center style={{ position: 'fixed', inset: 0, zIndex: 10000, background: 'var(--mantine-color-body)' }}>
      <AppLogo size={64} alt="" />
    </Center>
  )
}
