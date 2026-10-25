export const APP_COMMANDS = {
  search: 'find',
  settings: 'settings',
  sidebar: 'toggle-sidebar',
  refresh: 'refresh-all',
  lock: 'lock',
} as const

export function shortcutLabel(combo: string): string {
  const mac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform)

  return combo
    .split('+')
    .map((key) => {
      if (key.toLowerCase() === 'mod') {
        return mac ? '⌘' : 'Ctrl'
      }
      if (key.toLowerCase() === 'shift') {
        return mac ? '⇧' : 'Shift'
      }

      return key.toUpperCase()
    })
    .join(mac ? '' : '+')
}
