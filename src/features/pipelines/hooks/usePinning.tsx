import { IconPin, IconPinnedOff } from '@tabler/icons-react'

import type { ContextMenuItem } from '../../../components/common/ContextMenu'
import type { PinKind } from '../../../types/views'
import { pinKey, useBusyPins, usePins, useSetPin } from '../queries/usePipelineViewQueries'
import { isPinned } from '../utils/navTree'

type PinMenuItem = Extract<ContextMenuItem, { label: string }>

export function usePinning(): (kind: PinKind, ref: string) => PinMenuItem {
  const { data: pins = [] } = usePins()
  const { mutate: setPin } = useSetPin()
  const { isBusy, isBusyNow } = useBusyPins()

  return (kind: PinKind, ref: string): PinMenuItem => {
    const pinned = isPinned(pins, kind, ref)
    const key = pinKey(kind, ref)

    return {
      label: `${pinned ? 'Unpin' : 'Pin'} ${kind}`,
      icon: pinned ? <IconPinnedOff size={16} aria-hidden /> : <IconPin size={16} aria-hidden />,
      disabled: isBusy(key),
      onClick: () => {
        if (!isBusyNow(key)) {
          setPin({ kind, ref, pinned: !pinned })
        }
      },
    }
  }
}
