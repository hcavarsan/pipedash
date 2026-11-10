import { useLocalStorage } from '@mantine/hooks'

export const PROVIDERS_KEY = 'providers'
export const VIEWS_COLLAPSED_KEY = 'views:collapsed'

interface SidebarExpansionInput {
  searching: boolean
  activeKeys: string[] | null
  activeRootListed: boolean
}

interface SidebarExpansion {
  isOpen: (key: string) => boolean
  toggle: (key: string) => void
  viewsOpen: boolean
}

export function useSidebarExpansion({
  searching,
  activeKeys,
  activeRootListed,
}: SidebarExpansionInput): SidebarExpansion {
  const [expanded, setExpanded] = useLocalStorage<string[]>({
    key: 'pipedash-sidebar-expanded',
    defaultValue: [PROVIDERS_KEY],
  })

  return {
    isOpen: (key) => searching || expanded.includes(key) || activeKeys?.includes(key) === true,
    toggle: (key) =>
      setExpanded((current) => (current.includes(key) ? current.filter((k) => k !== key) : [...current, key])),
    viewsOpen: searching || !expanded.includes(VIEWS_COLLAPSED_KEY) || activeRootListed,
  }
}
