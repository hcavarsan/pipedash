import type { MouseEvent } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { providerPath, pushState } from '../../../components/layout/mobileChrome'
import { useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import type { ProviderSummary } from '../../../types/domain'
import { isOutageLevel } from '../../../utils/providerAttention'
import { useProviderSignIn } from './useProviderSignIn'

export function useProviderNavigation(): (provider: ProviderSummary, event?: MouseEvent) => void {
  const navigate = useNavigate()
  const location = useLocation()
  const openStatus = useNotificationCenterStore((state) => state.open)
  const signIn = useProviderSignIn()

  return (provider, event) => {
    if (
      event &&
      (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    ) {
      return
    }

    event?.preventDefault()
    const path = providerPath(provider.id)
    if (location.pathname + location.search !== path) {
      void navigate(path, { state: pushState(location.pathname, location.search, location.state) })
    }

    if (provider.recovery === 'sign_in') {
      signIn(provider.id)
    } else if (
      provider.last_fetch_status === 'error' ||
      provider.freshness.state === 'down' ||
      isOutageLevel(provider.health.level)
    ) {
      openStatus({ type: 'provider', providerId: provider.id })
    }
  }
}
