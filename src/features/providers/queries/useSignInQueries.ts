import {
  type UseMutationResult,
  type UseQueryResult,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'

import { REFRESH_INTERVALS } from '../../../constants/intervals'
import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { SignInStart, SignInStatus } from '../../../types/domain'

export function useSignInStatus(id: string | null): UseQueryResult<SignInStatus, Error> {
  return useQuery({
    queryKey: queryKeys.signIn.status(id ?? ''),
    queryFn: () => service.signInStatus(id ?? ''),
    enabled: id !== null,
    retry: false,
    staleTime: 0,
    refetchInterval: (query) =>
      query.state.data?.state === 'pending' || query.state.status === 'error'
        ? REFRESH_INTERVALS.SIGN_IN_STATUS
        : false,
  })
}

export function useStartSignIn(): UseMutationResult<
  SignInStart,
  Error,
  { providerType: string; config: Record<string, string> },
  unknown
> {
  return useMutation({
    mutationFn: ({ providerType, config }: { providerType: string; config: Record<string, string> }) =>
      service.startSignIn(providerType, config),
  })
}

export function useCompleteSignIn(): UseMutationResult<
  SignInStatus,
  Error,
  { id: string; redirectUrl: string },
  unknown
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, redirectUrl }: { id: string; redirectUrl: string }) => service.completeSignIn(id, redirectUrl),
    onSuccess: (status, { id }) => queryClient.setQueryData(queryKeys.signIn.status(id), status),
  })
}

export function useCancelSignIn(): UseMutationResult<void, Error, string, unknown> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: string) => service.cancelSignIn(id),
    onSettled: (_data, _error, id) => queryClient.removeQueries({ queryKey: queryKeys.signIn.status(id) }),
  })
}
