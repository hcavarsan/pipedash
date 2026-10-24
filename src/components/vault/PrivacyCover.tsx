import { Center } from '@mantine/core'
import { useSyncExternalStore } from 'react'

import { useVaultStatus } from '../../queries/useVaultQueries'
import { shouldShowPrivacyCover } from '../../utils/appLock'
import { isMobilePlatform } from '../../utils/platform'
import { AppLogo } from '../atoms/AppLogo'

const subscribe = (onChange: () => void) => {
  document.addEventListener('visibilitychange', onChange)

  return () => document.removeEventListener('visibilitychange', onChange)
}

const getVisibility = () => document.visibilityState

export function PrivacyCover() {
  const visibility = useSyncExternalStore(subscribe, getVisibility)
  const { data: vaultStatus } = useVaultStatus({ enabled: isMobilePlatform() })

  if (!shouldShowPrivacyCover(vaultStatus?.app_lock?.enabled === true, visibility)) {
    return null
  }

  return (
    <Center style={{ position: 'fixed', inset: 0, zIndex: 10000, background: 'var(--mantine-color-body)' }}>
      <AppLogo size={64} alt="" />
    </Center>
  )
}
