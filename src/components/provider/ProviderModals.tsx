import { Text } from '@mantine/core'
import { useCallback, useEffect } from 'react'

import { logger } from '../../lib/logger'
import { useProviderDetails } from '../../queries/useProviderDetailsQuery'
import { useAddProvider, useProviders, useRemoveProvider, useUpdateProvider } from '../../queries/useProvidersQueries'
import { useProviderStore } from '../../stores/providerStore'
import type { ProviderConfig } from '../../types'
import { openConfirm } from '../../utils/confirm'

import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { StandardModal } from '../common/StandardModal'

import { AddProviderModal } from './AddProviderModal'

export function useConfirmRemoveProvider() {
  const { mutateAsync: removeProvider } = useRemoveProvider()

  return useCallback(
    (id: number, name: string) => {
      openConfirm({
        title: 'Remove provider',
        children: (
          <Text size="sm">
            Remove &quot;{name}&quot; and its pipelines from Pipedash? This action cannot be undone.
          </Text>
        ),
        labels: { confirm: 'Remove provider', cancel: 'Cancel' },
        confirmProps: { color: 'red' },
        onConfirm: async () => {
          try {
            await removeProvider(id)
          } catch (err) {
            logger.error('ProviderModals', 'Failed to remove provider', err)
          }
        },
      })
    },
    [removeProvider]
  )
}

export function ProviderModals() {
  const addProviderModal = useProviderStore((s) => s.addProviderModal)
  const editProviderModal = useProviderStore((s) => s.editProviderModal)
  const editRequestId = useProviderStore((s) => s.editRequestId)
  const closeAddProviderModal = useProviderStore((s) => s.closeAddProviderModal)
  const openEditProviderModal = useProviderStore((s) => s.openEditProviderModal)
  const closeEditProviderModal = useProviderStore((s) => s.closeEditProviderModal)

  const { refetch: refetchProviders } = useProviders()
  const { mutateAsync: addProvider } = useAddProvider()
  const { mutateAsync: updateProvider } = useUpdateProvider()
  const {
    data: providerDetails,
    error: providerDetailsError,
    isError: providerDetailsFailed,
    isFetching: fetchingProviderDetails,
    refetch: refetchProviderDetails,
  } = useProviderDetails(editRequestId)

  useEffect(() => {
    if (providerDetails && editRequestId !== null) {
      openEditProviderModal(providerDetails)
    }
  }, [providerDetails, editRequestId, openEditProviderModal])

  const handleAdd = async (config: ProviderConfig) => {
    await addProvider(config)
  }

  const handleUpdate = async (id: number, config: ProviderConfig) => {
    await updateProvider({ id, config })
  }

  const handleCloseAdd = () => {
    closeAddProviderModal()
    refetchProviders()
  }

  return (
    <>
      <AddProviderModal opened={addProviderModal.open} onClose={handleCloseAdd} onAdd={handleAdd} />

      <StandardModal opened={editRequestId !== null} onClose={closeEditProviderModal} title="Edit provider" size="md">
        {providerDetailsFailed ? (
          <ErrorState
            title="Couldn't load provider"
            error={providerDetailsError}
            onRetry={() => refetchProviderDetails()}
            retrying={fetchingProviderDetails}
          />
        ) : (
          <LoadingState variant="section" message="Loading provider..." />
        )}
      </StandardModal>

      {editProviderModal.provider && (
        <AddProviderModal
          opened={editProviderModal.open}
          onClose={closeEditProviderModal}
          onUpdate={handleUpdate}
          editMode
          existingProvider={editProviderModal.provider}
        />
      )}
    </>
  )
}
