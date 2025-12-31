import { useCallback, useState } from 'react'

import { useProviderPermissions } from '../../../queries/useProvidersQueries'
import type { PluginMetadata, ProviderConfig } from '../../../types'
import { isAuthError, isNetworkError, isPermissionError, toPipedashError } from '../../../types/errors'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'

import { initialPermissionState, type PermissionState } from './types'

interface UsePermissionCheckParams {
  selectedPlugin: PluginMetadata | null
  providerConfig: ProviderConfig
}

interface UsePermissionCheckReturn extends PermissionState {
  checkPermissions: () => Promise<void>
  openModal: () => void
  closeModal: () => void
  reset: () => void
}

export function usePermissionCheck({
  selectedPlugin,
  providerConfig,
}: UsePermissionCheckParams): UsePermissionCheckReturn {
  const [state, setState] = useState<PermissionState>(initialPermissionState)

  const permissionsQuery = useProviderPermissions(
    selectedPlugin?.provider_type || '',
    providerConfig
  )

  const openModal = useCallback(() => {
    setState((prev) => ({ ...prev, modalOpen: true }))
  }, [])

  const closeModal = useCallback(() => {
    setState((prev) => ({ ...prev, modalOpen: false }))
  }, [])

  const reset = useCallback(() => {
    setState(initialPermissionState)
  }, [])

  const checkPermissions = useCallback(async () => {
    if (!selectedPlugin) {
      return
    }

    setState((prev) => ({
      ...prev,
      checking: true,
      modalOpen: true,
      error: null,
      status: null,
      features: [],
    }))

    try {
      const { data, error } = await permissionsQuery.refetch()

      if (error) {
        throw error
      }

      if (data) {
        setState((prev) => ({
          ...prev,
          status: data.permission_status,
          features: data.features,
          checking: false,
        }))

        if (data.permission_status?.all_granted) {
          displaySuccessNotification('All permissions granted')
        }
      }
    } catch (err: unknown) {
      const error = toPipedashError(err)
      let errorMessage = 'Failed to check permissions'

      if (isAuthError(error)) {
        errorMessage = 'Authentication failed. Please check your credentials.'
      } else if (isPermissionError(error)) {
        errorMessage = 'Insufficient permissions. Please grant the required permissions.'
      } else if (isNetworkError(error)) {
        errorMessage = 'Network error. Please check your connection.'
      } else {
        errorMessage = error.message
      }

      setState((prev) => ({
        ...prev,
        error: errorMessage,
        checking: false,
      }))

      displayErrorNotification(err, 'Permission Check Failed')
    }
  }, [selectedPlugin, permissionsQuery])

  return {
    ...state,
    checkPermissions,
    openModal,
    closeModal,
    reset,
  }
}
