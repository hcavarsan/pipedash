import { Center } from '@mantine/core'
import { type ReactNode, useEffect, useSyncExternalStore } from 'react'

import { logger } from '../../../lib/logger'
import { useVaultStatus } from '../../../queries/useVaultQueries'
import { mobileService } from '../../../services/mobile'
import { shouldShowPrivacyCover } from '../../../utils/appLock'
import { isMobilePlatform } from '../../../utils/platform'

const subscribe = (onChange: () => void) => {
  document.addEventListener('visibilitychange', onChange)

  return () => document.removeEventListener('visibilitychange', onChange)
}

const getVisibility = () => document.visibilityState

export function PrivacyCover(): ReactNode {
  const visibility = useSyncExternalStore(subscribe, getVisibility)
  const { data: vaultStatus, refetch } = useVaultStatus({ enabled: isMobilePlatform() })
  const appLock = vaultStatus?.app_lock

  useEffect(() => {
    if (visibility === 'visible' && isMobilePlatform()) {
      void refetch().finally(() => {
        mobileService.releasePrivacyCover().catch((error: unknown) => {
          logger.error('PrivacyCover', 'Failed to release privacy cover', error)
        })
      })
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
