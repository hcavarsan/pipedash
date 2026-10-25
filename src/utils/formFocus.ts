import { type KeyboardEvent, type RefObject, useCallback, useEffect, useState } from 'react'

import { isMobileShell } from '../hooks/useIsMobile'
import { scrollBehavior } from '../lib/motion'

const TEXT_FIELDS =
  'input:not([type="hidden"], [type="checkbox"], [type="radio"], [type="button"], [type="submit"]), textarea'

export function handleFormEnter(event: KeyboardEvent<HTMLFormElement>) {
  const { target, currentTarget } = event

  if (
    !isMobileShell() ||
    event.key !== 'Enter' ||
    event.defaultPrevented ||
    event.nativeEvent.isComposing ||
    !(target instanceof HTMLInputElement) ||
    target.hasAttribute('aria-haspopup')
  ) {
    return
  }

  const fields = Array.from(currentTarget.querySelectorAll<HTMLElement>(TEXT_FIELDS)).filter(
    (field) => !field.hasAttribute('disabled') && field.getClientRects().length > 0
  )
  const next = fields[fields.indexOf(target) + 1]

  event.preventDefault()

  if (next) {
    next.focus()
  } else {
    currentTarget.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
  }
}

export function useFocusFirstInvalid(rootRef: RefObject<HTMLElement | null>) {
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
