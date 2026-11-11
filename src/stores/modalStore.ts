import { create } from 'zustand'
import { devtools } from 'zustand/middleware'

import { closeSheetsFrom, sheetMark } from '../hooks/useSheetStack'

export type TriggerModalState = { open: false } | { open: true; pipelineId: string; rerunOf: number | undefined }

export type LogsModalState = { open: false } | { open: true; pipelineId: string; runNumber: number }

interface ModalStoreState {
  triggerModal: TriggerModalState
  logsModal: LogsModalState
}

interface ModalStoreActions {
  openTriggerModal: (pipelineId: string, rerunOf?: number) => void
  closeTriggerModal: () => void
  openLogsModal: (pipelineId: string, runNumber: number) => void
  showRunAfterAction: (pipelineId: string, runNumber: number) => void
  closeLogsModal: () => void
}

type ModalStore = ModalStoreState & ModalStoreActions

const initialState: ModalStoreState = {
  triggerModal: { open: false },
  logsModal: { open: false },
}

let dialogMark = 0

export const useModalStore = create<ModalStore>()(
  devtools(
    (set, get) => ({
      ...initialState,

      openTriggerModal: (pipelineId, rerunOf) => {
        if (!get().triggerModal.open && !get().logsModal.open) {
          dialogMark = sheetMark()
        }

        set({
          triggerModal: { open: true, pipelineId, rerunOf },
        })
      },

      closeTriggerModal: () => set({ triggerModal: { open: false } }),

      openLogsModal: async (pipelineId, runNumber) => {
        const { triggerModal, logsModal } = get()
        const same = logsModal.open && logsModal.pipelineId === pipelineId && logsModal.runNumber === runNumber

        if (same && !triggerModal.open) {
          return
        }

        if ((triggerModal.open || logsModal.open) && !(await closeSheetsFrom(dialogMark))) {
          return
        }

        dialogMark = sheetMark()
        set({ triggerModal: { open: false }, logsModal: { open: true, pipelineId, runNumber } })
      },

      showRunAfterAction: (pipelineId, runNumber) =>
        set({ triggerModal: { open: false }, logsModal: { open: true, pipelineId, runNumber } }),

      closeLogsModal: () => set({ logsModal: { open: false } }),
    }),
    { name: 'ModalStore' }
  )
)
