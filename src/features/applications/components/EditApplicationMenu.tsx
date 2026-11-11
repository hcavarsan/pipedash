import { Button, Menu } from '@mantine/core'
import { IconChevronDown, IconPencil } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'

import type { ApplicationCopy, ApplicationDetail } from '../../../types/applications'
import { openConfirm } from '../../../utils/confirm'
import { useDeleteApplication, useEditApplication } from '../queries/useApplicationQueries'
import type { ApplicationModalProps } from './applicationModalProps'
import { MembersModal } from './MembersModal'
import { MergeModal } from './MergeModal'
import { RenameModal } from './RenameModal'
import { SplitModal } from './SplitModal'
import { TargetsModal } from './TargetsModal'

type ModalKind = 'rename' | 'merge' | 'split' | 'members' | 'targets'

const MODALS: Record<ModalKind, (props: ApplicationModalProps) => ReactNode> = {
  rename: RenameModal,
  merge: MergeModal,
  split: SplitModal,
  members: MembersModal,
  targets: TargetsModal,
}

interface EditApplicationMenuProps {
  detail: ApplicationDetail
  copy: ApplicationCopy
  onDeleted: () => void
}

export function EditApplicationMenu({ detail, copy, onDeleted }: EditApplicationMenuProps): ReactNode {
  const [modal, setModal] = useState<ModalKind | null>(null)
  const edit = useEditApplication()
  const remove = useDeleteApplication()
  const { id, hidden, name, capabilities } = detail.summary
  const Modal = modal === null ? null : MODALS[modal]

  const confirmDelete = (): void => {
    openConfirm({
      title: `${copy.delete}: ${name}`,
      children: name,
      labels: { confirm: copy.delete, cancel: copy.cancel },
      confirmProps: { color: 'red' },
      onConfirm: () => remove.mutate(id, { onSuccess: onDeleted }),
    })
  }

  return (
    <>
      <Menu position="bottom-end">
        <Menu.Target>
          <Button
            size="xs"
            variant="default"
            leftSection={<IconPencil size={14} aria-hidden />}
            rightSection={<IconChevronDown size={14} aria-hidden />}
          >
            {copy.edit}
          </Button>
        </Menu.Target>
        <Menu.Dropdown>
          <Menu.Item onClick={() => setModal('rename')}>{copy.rename}</Menu.Item>
          <Menu.Item onClick={() => setModal('members')}>{copy.members}</Menu.Item>
          <Menu.Item onClick={() => setModal('targets')}>{copy.targets}</Menu.Item>
          <Menu.Item onClick={() => setModal('merge')}>{copy.merge}</Menu.Item>
          <Menu.Item onClick={() => setModal('split')}>{copy.split}</Menu.Item>
          <Menu.Item onClick={() => edit.mutate({ id, edit: { type: 'hide', hidden: !hidden } })}>
            {hidden ? copy.unhide : copy.hide}
          </Menu.Item>
          {capabilities.delete && (
            <Menu.Item color="red" onClick={confirmDelete}>
              {copy.delete}
            </Menu.Item>
          )}
        </Menu.Dropdown>
      </Menu>
      {Modal && <Modal detail={detail} copy={copy} onClose={() => setModal(null)} />}
    </>
  )
}
