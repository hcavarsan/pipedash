import { useCallback, useEffect, useLayoutEffect, useRef } from 'react'

import { useMobileNavStore } from '../../stores/mobileNavStore'
import type { ElementRef } from './useMobilePageMotion'

export function useMobilePageArrival(page: ElementRef, announcement: ElementRef, title: string): () => void {
  const latestTitle = useRef(title)
  const frame = useRef(0)

  useLayoutEffect(() => {
    latestTitle.current = title
  }, [title])

  useEffect(
    () => () => {
      cancelAnimationFrame(frame.current)
    },
    []
  )

  return useCallback(() => {
    const heading =
      page.current?.querySelector<HTMLElement>('[data-page-heading]') ??
      document.querySelector<HTMLElement>('[data-bar-live] [data-page-heading]')

    heading?.focus({ preventScroll: true })
    cancelAnimationFrame(frame.current)

    const region = announcement.current

    if (region) {
      region.textContent = ''
      frame.current = requestAnimationFrame(() => {
        region.textContent = useMobileNavStore.getState().title ?? latestTitle.current
      })
    }
  }, [page, announcement])
}
