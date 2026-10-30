import type { QueryClient } from '@tanstack/react-query'

import { logger } from '../../../lib/logger'
import { queryKeys } from '../../../lib/queryKeys'
import { invalidateProviderPipelines } from '../../../queries/usePipelinesQueries'
import { service } from '../../../services/service'
import { useProviderStore } from '../../../stores/providerStore'
import { errorMessage } from '../../../utils/errorDisplay'

let nextRequestId = 0

function discardFlow(id: string): void {
  void service.cancelSignIn(id).catch((error: unknown) => {
    logger.error('ProviderSignIn', 'Failed to cancel sign-in', error)
  })
}

function startProviderSignIn(providerId: number): void {
  if (useProviderStore.getState().providerSignIn !== null) {
    return
  }

  const requestId = ++nextRequestId

  useProviderStore.setState({
    providerSignIn: { requestId, providerId, flow: null, phase: 'starting', error: null },
  })

  void service.startProviderSignIn(providerId).then(
    (flow) => {
      const session = useProviderStore.getState().providerSignIn

      if (session?.requestId !== requestId) {
        discardFlow(flow.id)

        return
      }

      useProviderStore.setState({ providerSignIn: { ...session, flow, phase: 'waiting' } })
    },
    (error: unknown) => {
      const session = useProviderStore.getState().providerSignIn

      if (session?.requestId === requestId) {
        useProviderStore.setState({
          providerSignIn: { ...session, phase: 'failed', error: errorMessage(error, "Couldn't start the sign-in") },
        })
      }
    }
  )
}

export function useProviderSignIn(): (providerId: number) => void {
  return startProviderSignIn
}

export function cancelProviderSignIn(unmount = false): void {
  const session = useProviderStore.getState().providerSignIn

  if (!session || (session.phase === 'saving' && !unmount)) {
    return
  }

  useProviderStore.setState({ providerSignIn: null })

  if (session.flow && session.phase !== 'saving') {
    discardFlow(session.flow.id)
  }
}

export function reconnectProviderSignIn(queryClient: QueryClient, requestId: number): void {
  const session = useProviderStore.getState().providerSignIn

  if (
    session?.requestId !== requestId ||
    !session.flow ||
    (session.phase !== 'waiting' && session.phase !== 'save_failed')
  ) {
    return
  }

  const { providerId, flow } = session

  useProviderStore.setState({ providerSignIn: { ...session, phase: 'saving', error: null } })

  void service.reconnectProvider(providerId, flow.id).then(
    () => {
      queryClient.removeQueries({ queryKey: queryKeys.signIn.status(flow.id) })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.detail(providerId) })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.permissions(providerId) })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.features(providerId) })
      invalidateProviderPipelines(queryClient, providerId)

      if (useProviderStore.getState().providerSignIn?.requestId === requestId) {
        useProviderStore.setState({ providerSignIn: null })
      }
    },
    (error: unknown) => {
      const current = useProviderStore.getState().providerSignIn

      if (current?.requestId === requestId) {
        useProviderStore.setState({
          providerSignIn: {
            ...current,
            phase: 'save_failed',
            error: errorMessage(error, "Couldn't save the new sign-in. Try again."),
          },
        })
      } else {
        discardFlow(flow.id)
      }
    }
  )
}
