import { IconPin, IconPinnedOff } from '@tabler/icons-react'

import type { ContextMenuItem } from '../components/common/ContextMenu'
import { usePins, useSetPin } from '../queries/usePipelineViewQueries'
import type { PinKind } from '../types'
import { isPinned } from '../utils/navTree'

type PinMenuItem = Extract<ContextMenuItem, { label: string }>

export function usePinning(): (kind: PinKind, ref: string) => PinMenuItem {
  const { data: pins = [] } = usePins()
  const { mutate: setPin } = useSetPin()

  return (kind: PinKind, ref: string): PinMenuItem => {
    const pinned = isPinned(pins, kind, ref)

    return {
      label: `${pinned ? 'Unpin' : 'Pin'} ${kind}`,
      icon: pinned ? <IconPinnedOff size={16} /> : <IconPin size={16} />,
      onClick: () => setPin({ kind, ref, pinned: !pinned }),
    }
  }
}
