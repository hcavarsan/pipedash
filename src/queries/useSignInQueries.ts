import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { REFRESH_INTERVALS } from '../constants/intervals'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services'

export function useSignInStatus(id: string | null) {
  return useQuery({
    queryKey: queryKeys.signIn.status(id ?? ''),
    queryFn: () => service.signInStatus(id ?? ''),
    enabled: id !== null,
    retry: false,
    staleTime: 0,
    refetchInterval: (query) => (query.state.data?.state === 'pending' ? REFRESH_INTERVALS.SIGN_IN_STATUS : false),
  })
}

export function useStartSignIn() {
  return useMutation({
    mutationFn: ({ providerType, config }: { providerType: string; config: Record<string, string> }) =>
      service.startSignIn(providerType, config),
  })
}

export function useCompleteSignIn() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, redirectUrl }: { id: string; redirectUrl: string }) => service.completeSignIn(id, redirectUrl),
    onSuccess: (status, { id }) => queryClient.setQueryData(queryKeys.signIn.status(id), status),
  })
}

export function useCancelSignIn() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: string) => service.cancelSignIn(id),
    onSettled: (_data, _error, id) => queryClient.removeQueries({ queryKey: queryKeys.signIn.status(id) }),
  })
}
