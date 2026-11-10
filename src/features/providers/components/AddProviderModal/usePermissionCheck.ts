import { useState } from 'react'

import { useProviderPermissions } from '../../../../queries/useProvidersQueries'
import { isAuthError, isNetworkError, isPermissionError, toPipedashError } from '../../../../types/errors'
import type { FeatureAvailability, PermissionStatus, PluginMetadata } from '../../../../types/plugins'
import type { ProviderConfig } from '../../../../types/providers'

interface UsePermissionCheckParams {
  selectedPlugin: PluginMetadata | null
  providerConfig: ProviderConfig
}

interface UsePermissionCheckResult {
  modalOpen: boolean
  status: PermissionStatus | null
  features: FeatureAvailability[]
  checking: boolean
  error: string | null
  checkPermissions: () => Promise<void>
  closeModal: () => void
}

const NO_FEATURES: FeatureAvailability[] = []

export function permissionErrorMessage(err: unknown): string {
  const error = toPipedashError(err)

  if (isAuthError(error)) {
    return "Couldn't verify credentials. Check your token or connection settings."
  }

  if (isPermissionError(error)) {
    return "Couldn't verify permissions. Grant the required permissions."
  }

  if (isNetworkError(error)) {
    return "Couldn't reach the provider. Check your connection."
  }

  return error.message || "Couldn't check permissions"
}

export function usePermissionCheck({
  selectedPlugin,
  providerConfig,
}: UsePermissionCheckParams): UsePermissionCheckResult {
  const [modalOpen, setModalOpen] = useState(false)
  const query = useProviderPermissions(selectedPlugin?.provider_type || '', providerConfig)
  const { refetch } = query

  const checkPermissions = async (): Promise<void> => {
    if (!selectedPlugin) {
      return
    }

    setModalOpen(true)

    await refetch()
  }

  return {
    modalOpen,
    status: query.data?.permission_status ?? null,
    features: query.data?.features ?? NO_FEATURES,
    checking: query.isFetching,
    error: query.isFetching || !query.error ? null : permissionErrorMessage(query.error),
    checkPermissions,
    closeModal: () => setModalOpen(false),
  }
}
