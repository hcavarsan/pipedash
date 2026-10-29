import { type RefObject, useEffect } from 'react'

import { isMobileShell } from './useIsMobile'

const TEXT_FIELDS =
  'input:not([type="hidden"], [type="checkbox"], [type="radio"], [type="button"], [type="submit"]), textarea'

export function useFormEnterNavigation(formRef: RefObject<HTMLFormElement | null>): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      const form = formRef.current
      const { target } = event

      if (
        !form ||
        !isMobileShell() ||
        event.key !== 'Enter' ||
        event.defaultPrevented ||
        event.isComposing ||
        !(target instanceof HTMLInputElement) ||
        !form.contains(target) ||
        target.hasAttribute('aria-haspopup')
      ) {
        return
      }

      const fields = Array.from(form.querySelectorAll<HTMLElement>(TEXT_FIELDS)).filter(
        (field) => !field.hasAttribute('disabled') && field.getClientRects().length > 0
      )
      const next = fields[fields.indexOf(target) + 1]

      event.preventDefault()

      if (next) {
        next.focus()
      } else {
        form.requestSubmit()
      }
    }

    document.addEventListener('keydown', onKeyDown)

    return () => {
      document.removeEventListener('keydown', onKeyDown)
    }
  }, [formRef])
}
