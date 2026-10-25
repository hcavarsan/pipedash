import { SETTINGS_SECTIONS } from '../settings/SettingsSidebar'

export type MobileTab = 'pipelines' | 'pinned' | 'providers' | 'notifications' | 'settings'

export interface MobileChrome {
  title: string
  back: { to: string; label: string } | null
  tab: MobileTab
}

export function mobileChrome(pathname: string, search: string): MobileChrome {
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
      const pinned = !segment && new URLSearchParams(search).get('view') === 'pinned'

      return { title: pinned ? 'Pinned' : 'Pipelines', back: null, tab: pinned ? 'pinned' : 'pipelines' }
    }
  }
}
