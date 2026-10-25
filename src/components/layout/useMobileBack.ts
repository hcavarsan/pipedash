import { useLocation } from 'react-router-dom'

import { mobileChrome } from './mobileChrome'

export interface BackTarget {
  to: string
  label: string
}

export function useMobileBack(): BackTarget | null {
  const { pathname, search, state } = useLocation()
  const { back } = mobileChrome(pathname, search)

  if (!back) {
    return null
  }

  const to = typeof state?.from === 'string' ? state.from : back.to

  return { to, label: to.includes('view=pinned') ? 'Pinned' : back.label }
}
