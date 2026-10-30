import { STATUS_META } from '../../theme/status'
import { typedKeys } from '../../utils/typedKeys'
import { SETTINGS_SECTIONS } from '../common/SettingsSidebar'

const MOBILE_TABS = ['pipelines', 'pinned', 'notifications', 'settings'] as const

export const BROWSE_PATH = '/browse'

const BROWSE_BACK = { to: BROWSE_PATH, label: 'Browse' }
const LIST_BACK = { to: '/pipelines', label: 'All pipelines' }

export type MobileTab = (typeof MOBILE_TABS)[number]

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

function tabOf(state: unknown): MobileTab | null {
  if (typeof state === 'object' && state !== null && 'tab' in state) {
    const { tab } = state

    return MOBILE_TABS.find((item) => item === tab) ?? null
  }

  return null
}

function depthOf(state: unknown): number | null {
  if (typeof state === 'object' && state !== null && 'depth' in state) {
    const { depth } = state

    return typeof depth === 'number' && Number.isInteger(depth) && depth > 0 ? depth : null
  }

  return null
}

export const groupPath = (sectionId: string): string => `/pipelines/groups/${encodeURIComponent(sectionId)}`

export const providerPath = (providerId: number): string => `/pipelines?provider=${providerId}`

function groupTitle(sectionId: string): string {
  const at = sectionId.indexOf(':')
  const kind = sectionId.slice(0, at)
  const ref = sectionId.slice(at + 1)

  switch (kind) {
    case 'repo':
      return ref.slice(ref.lastIndexOf('/') + 1)
    case 'org':
      return ref
    case 'status': {
      const status = typedKeys(STATUS_META).find((key) => key === ref)

      return status ? STATUS_META[status].label : ref
    }
    case 'provider':
      return 'Provider'
    default:
      return 'View'
  }
}

function ownChrome(pathname: string, search: string): MobileChrome {
  const [, root, segment, group] = pathname.split('/')

  switch (root) {
    case 'settings': {
      const section = SETTINGS_SECTIONS.find((item) => item.id === segment)

      return section
        ? { title: section.label, back: { to: '/settings', label: 'Settings' }, tab: 'settings' }
        : { title: 'Settings', back: null, tab: 'settings' }
    }
    case 'browse':
      return { title: 'Browse', back: null, tab: 'pipelines' }
    case 'notifications':
      return { title: 'Notifications', back: null, tab: 'notifications' }
    default: {
      if (root === 'pipelines' && segment === 'groups' && group) {
        return { title: groupTitle(decodeURIComponent(group)), back: LIST_BACK, tab: 'pipelines' }
      }

      if (root === 'pipelines' && segment) {
        return { title: 'History', back: LIST_BACK, tab: 'pipelines' }
      }

      const params = new URLSearchParams(search)

      if (params.get('view') === 'pinned') {
        return { title: 'Pinned', back: null, tab: 'pinned' }
      }

      const title = params.has('provider') ? 'Provider' : params.has('view') ? 'View' : 'All pipelines'

      return { title, back: BROWSE_BACK, tab: 'pipelines' }
    }
  }
}

export function mobileChrome(pathname: string, search: string, state?: unknown): MobileChrome {
  const own = ownChrome(pathname, search)
  const from = fromOf(state)

  if (!own.back) {
    return own
  }

  if (!from || from === pathname + search) {
    return { ...own, tab: tabOf(state) ?? own.tab }
  }

  const origin = splitLocation(from)
  const parent = ownChrome(origin.pathname, origin.search)

  return { ...own, back: { to: from, label: parent.title }, tab: tabOf(state) ?? parent.tab }
}

export interface NavState {
  from: string
  tab: MobileTab
  depth: number
}

export function screenDepth(pathname: string, search: string, state?: unknown): number {
  return depthOf(state) ?? (ownChrome(pathname, search).back ? 1 : 0)
}

export function pushState(pathname: string, search: string, state?: unknown): NavState {
  return {
    from: pathname + search,
    tab: mobileChrome(pathname, search, state).tab,
    depth: screenDepth(pathname, search, state) + 1,
  }
}

export function routeKey(pathname: string, search: string, state?: unknown): string {
  return `${mobileChrome(pathname, search, state).tab}|${pathname}`
}
