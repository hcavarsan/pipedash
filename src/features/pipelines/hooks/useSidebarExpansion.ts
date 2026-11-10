import { useLocalStorage } from '@mantine/hooks'
import { useEffect } from 'react'

export const PROVIDERS_KEY = 'providers'
export const VIEWS_COLLAPSED_KEY = 'views:collapsed'

const KEY_SEPARATOR = '\n'

interface SidebarExpansionInput {
  searching: boolean
  viewId: string | null
  revealKeys: string[]
  revealViews: boolean
}

interface SidebarExpansion {
  isOpen: (key: string) => boolean
  toggle: (key: string) => void
  viewsOpen: boolean
}

export function useSidebarExpansion({
  searching,
  viewId,
  revealKeys,
  revealViews,
}: SidebarExpansionInput): SidebarExpansion {
  const [expanded, setExpanded] = useLocalStorage<string[]>({
    key: 'pipedash-sidebar-expanded',
    defaultValue: [PROVIDERS_KEY],
  })
  const revealKey = revealKeys.join(KEY_SEPARATOR)

  useEffect(() => {
    if (viewId === null) {
      return
    }
    const keys = revealKey === '' ? [] : revealKey.split(KEY_SEPARATOR)

    setExpanded((current) => {
      const missing = keys.filter((key) => !current.includes(key))
      const hidden = revealViews && current.includes(VIEWS_COLLAPSED_KEY)

      if (missing.length === 0 && !hidden) {
        return current
      }

      return [...current, ...missing].filter((key) => !revealViews || key !== VIEWS_COLLAPSED_KEY)
    })
  }, [viewId, revealKey, revealViews, setExpanded])

  return {
    isOpen: (key) => searching || expanded.includes(key),
    toggle: (key) =>
      setExpanded((current) => (current.includes(key) ? current.filter((k) => k !== key) : [...current, key])),
    viewsOpen: searching || !expanded.includes(VIEWS_COLLAPSED_KEY),
  }
}
