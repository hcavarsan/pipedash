import { notifications } from '@mantine/notifications'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useEffectEvent, useRef } from 'react'
import { useSearchParams } from 'react-router-dom'

import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import type { ProviderSummary } from '../types/domain'

export function useProviderFilterReconciliation(): void {
  const queryClient = useQueryClient()
  const [searchParams, setSearchParams] = useSearchParams()
  const providerParam = searchParams.get('provider')
  const handled = useRef<string | null>(null)

  const reconcile = useEffectEvent((param: string | null) => {
    if (!param) {
      handled.current = null

      return
    }

    if (handled.current === param || queryClient.getQueryState(queryKeys.providers.list())?.status !== 'success') {
      return
    }

    const providers = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list()) ?? []

    if (providers.some((provider) => String(provider.id) === param)) {
      return
    }

    handled.current = param
    logger.info('App', 'Selected provider no longer exists, clearing filter', { providerParam: param })

    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev)

        next.delete('provider')

        return next
      },
      { replace: true }
    )

    notifications.show({
      title: 'Filter cleared',
      message: 'The selected provider no longer exists.',
      color: 'gray',
    })
  })

  useEffect(() => {
    reconcile(providerParam)

    return queryClient.getQueryCache().subscribe((event) => {
      if (event.type === 'updated') {
        reconcile(providerParam)
      }
    })
  }, [queryClient, providerParam])
}
