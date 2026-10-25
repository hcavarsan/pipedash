import { useEffect } from 'react'

import { useMobileShell } from './useIsMobile'

export function useDocumentTitle(title: string) {
  const mobileShell = useMobileShell()

  useEffect(() => {
    if (!mobileShell) {
      document.title = `${title} · Pipedash`
    }
  }, [title, mobileShell])
}
