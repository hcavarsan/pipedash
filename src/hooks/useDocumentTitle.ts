import { useEffect } from 'react'

import { useMobileShell } from './useIsMobile'

export function useDocumentTitle(title: string): void {
  const mobileShell = useMobileShell()

  useEffect(() => {
    if (!mobileShell) {
      document.title = `${title} · Pipedash`
    }
  }, [title, mobileShell])
}
