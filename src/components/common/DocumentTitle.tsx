import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'

export function DocumentTitle({ title }: { title: string }): ReactNode {
  const mobileShell = useMobileShell()

  return mobileShell ? null : <title>{`${title} · Pipedash`}</title>
}
