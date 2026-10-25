import { useCallback, useLayoutEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { mobileChrome } from './mobileChrome'
import { historyIndex, previousEntry, recordEntry, resolveBack } from './navigation'

export interface BackTarget {
  to: string
  label: string
}

export function useMobileBack(): BackTarget | null {
  const { pathname, search, state } = useLocation()

  return mobileChrome(pathname, search, state).back
}

export function useRecordHistory(): void {
  const { pathname, search } = useLocation()

  useLayoutEffect(() => {
    recordEntry(historyIndex(), pathname + search)
  }, [pathname, search])
}

export function useGoBack(): (parent: string) => void {
  const navigate = useNavigate()

  return useCallback(
    (parent) => {
      const index = historyIndex()

      if (resolveBack(index, previousEntry(index), parent) === 'pop') {
        navigate(-1)
      } else {
        navigate(parent, { replace: true })
      }
    },
    [navigate]
  )
}
