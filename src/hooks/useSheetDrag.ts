import { useEffect } from 'react'

import { startSheetDragTracking } from '../lib/sheetDrag'

export function useSheetDrag(active: boolean): void {
  useEffect(() => {
    if (!active) {
      return
    }

    return startSheetDragTracking(document)
  }, [active])
}
