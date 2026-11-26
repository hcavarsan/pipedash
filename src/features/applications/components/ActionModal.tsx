import { Button, Group } from '@mantine/core'
import { type ReactNode, useId } from 'react'

import { StandardModal } from '../../../components/common/StandardModal'
import type { ModalAction } from '../../../components/common/standardModalTypes'

interface ActionModalProps {
  title: string
  cancel: string
  action: ModalAction
  onClose: () => void
  loading?: boolean
  dirty?: boolean
  children: ReactNode
}

export function ActionModal({ title, cancel, action, onClose, loading, dirty, children }: ActionModalProps): ReactNode {
  const formId = useId()
  const pending = Boolean(action.loading)
  const disabled = Boolean(action.disabled)
  const blocked = disabled || pending

  return (
    <StandardModal
      opened
      onClose={onClose}
      size="md"
      title={title}
      loading={Boolean(loading) || pending}
      dirty={Boolean(dirty)}
      primaryAction={action}
      footer={(requestClose) => (
        <Group justify="flex-end" gap="sm">
          <Button variant="default" size="sm" onClick={requestClose} disabled={pending}>
            {cancel}
          </Button>
          <Button type="submit" form={formId} size="sm" loading={pending} disabled={disabled}>
            {action.label}
          </Button>
        </Group>
      )}
    >
      <form
        id={formId}
        onSubmit={(event) => {
          event.preventDefault()

          if (!blocked) {
            action.onClick()
          }
        }}
      >
        {children}
      </form>
    </StandardModal>
  )
}
