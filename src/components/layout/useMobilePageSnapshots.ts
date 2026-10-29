import { type RefObject, useEffect } from 'react'

import { captureSnapshot } from './snapshots'
import type { ElementRef } from './useMobilePageMotion'

const CAPTURE_DELAY_MS = 600

interface MobilePageSnapshotsProps {
  content: ElementRef
  routeKey: string
  scrollTop: RefObject<number>
  collapse: RefObject<number>
}

export function useMobilePageSnapshots({ content, routeKey, scrollTop, collapse }: MobilePageSnapshotsProps): void {
  useEffect(() => {
    const node = content.current

    if (!node) {
      return
    }

    let timer = 0
    let dirty = false

    const flush = () => {
      window.clearTimeout(timer)
      if (dirty) {
        dirty = false
        captureSnapshot(routeKey, node, scrollTop.current, collapse.current)
      }
    }
    const schedule = () => {
      dirty = true
      window.clearTimeout(timer)
      timer = window.setTimeout(flush, CAPTURE_DELAY_MS)
    }
    const observer = new MutationObserver((records) => {
      if (records.some((record) => record.target !== node)) {
        schedule()
      }
    })

    schedule()
    observer.observe(node, { subtree: true, childList: true, characterData: true, attributes: true })
    document.addEventListener('click', flush, { capture: true, passive: true })

    return () => {
      window.clearTimeout(timer)
      observer.disconnect()
      document.removeEventListener('click', flush, { capture: true })
    }
  }, [content, routeKey, scrollTop, collapse])
}
