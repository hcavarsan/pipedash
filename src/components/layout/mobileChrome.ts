import { SETTINGS_SECTIONS } from '../common/SettingsSidebar'

export type MobileTab = 'pipelines' | 'pinned' | 'providers' | 'notifications' | 'settings'

export interface MobileChrome {
  title: string
  back: { to: string; label: string } | null
  tab: MobileTab
}

export function splitLocation(location: string): { pathname: string; search: string } {
  const at = location.indexOf('?')

  return at < 0 ? { pathname: location, search: '' } : { pathname: location.slice(0, at), search: location.slice(at) }
}

export function fromOf(state: unknown): string | null {
  if (typeof state === 'object' && state !== null && 'from' in state) {
    const { from } = state

    return typeof from === 'string' && from.startsWith('/') ? from : null
  }

  return null
}

function ownChrome(pathname: string, search: string): MobileChrome {
  const [, root, segment] = pathname.split('/')

  switch (root) {
    case 'settings': {
      const section = SETTINGS_SECTIONS.find((item) => item.id === segment)

      return section
        ? { title: section.label, back: { to: '/settings', label: 'Settings' }, tab: 'settings' }
        : { title: 'Settings', back: null, tab: 'settings' }
    }
    case 'providers':
      return { title: 'Providers', back: null, tab: 'providers' }
    case 'notifications':
      return { title: 'Notifications', back: null, tab: 'notifications' }
    default: {
      if (root === 'pipelines' && segment) {
        return { title: 'History', back: { to: '/pipelines', label: 'Pipelines' }, tab: 'pipelines' }
      }

      const pinned = !segment && new URLSearchParams(search).get('view') === 'pinned'

      return { title: pinned ? 'Pinned' : 'Pipelines', back: null, tab: pinned ? 'pinned' : 'pipelines' }
    }
  }
}

export function mobileChrome(pathname: string, search: string, state?: unknown): MobileChrome {
  const own = ownChrome(pathname, search)
  const from = fromOf(state)

  if (!own.back || !from || from === pathname + search) {
    return own
  }

  const origin = splitLocation(from)
  const parent = ownChrome(origin.pathname, origin.search)

  return { ...own, back: { to: from, label: parent.title }, tab: parent.tab }
}

export function routeKey(pathname: string, search: string, state?: unknown): string {
  return `${mobileChrome(pathname, search, state).tab}|${pathname}`
}
