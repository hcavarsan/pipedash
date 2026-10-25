import { scrollBehavior } from '../../lib/motion'

export function scrollActiveToTop(): void {
  document.querySelector('.mantine-AppShell-main')?.scrollTo({ top: 0, behavior: scrollBehavior() })
}
