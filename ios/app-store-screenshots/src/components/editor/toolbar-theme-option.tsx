import type { ReactNode } from 'react'
import type { Theme } from '@/lib/types'

export function ThemeOption({ theme }: { theme: Theme }): ReactNode {
  return (
    <span className="flex min-w-0 items-center gap-2">
      <span aria-hidden className="flex h-4 w-7 shrink-0 overflow-hidden rounded-sm ring-1 ring-black/10">
        <span className="flex-1" style={{ background: theme.bg }} />
        <span className="flex-1" style={{ background: theme.bgAlt }} />
        <span className="w-1.5" style={{ background: theme.accent }} />
      </span>
      <span className="truncate">{theme.name}</span>
    </span>
  )
}
