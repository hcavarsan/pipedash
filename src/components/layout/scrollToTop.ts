import { scrollBehavior } from '../../lib/motion'

interface ScrollTopEntry {
  isActive: () => boolean
  scrollToTop: () => void
}

const entries: ScrollTopEntry[] = []

export function registerScrollToTop(entry: ScrollTopEntry): () => void {
  entries.push(entry)

  return () => {
    const index = entries.lastIndexOf(entry)

    if (index >= 0) {
      entries.splice(index, 1)
    }
  }
}

export function scrollActiveToTop(): void {
  const active = entries.findLast((entry) => entry.isActive())

  if (active) {
    active.scrollToTop()

    return
  }

  document.querySelector('.mantine-AppShell-main')?.scrollTo({ top: 0, behavior: scrollBehavior() })
}
