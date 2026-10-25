import { useCallback } from 'react'

import { confirmDiscard } from '../utils/confirm'

export function useDiscardGuard(dirty: boolean, onClose: () => void) {
  return useCallback(async () => {
    if (!dirty || (await confirmDiscard())) {
      onClose()
    }
  }, [dirty, onClose])
}
