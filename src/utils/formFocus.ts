import { type RefObject, useCallback, useEffect, useState } from 'react'

import { scrollBehavior } from '../lib/motion'

export function useFocusFirstInvalid(rootRef: RefObject<HTMLElement | null>): () => void {
  const [request, setRequest] = useState(0)

  useEffect(() => {
    if (request === 0) {
      return
    }

    const target = rootRef.current?.querySelector<HTMLElement>('[aria-invalid="true"]')

    target?.focus({ preventScroll: true })
    target?.scrollIntoView({ block: 'center', behavior: scrollBehavior() })
  }, [request, rootRef])

  return useCallback(() => setRequest((count) => count + 1), [])
}
