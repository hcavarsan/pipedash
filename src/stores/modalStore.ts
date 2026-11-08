import { create } from 'zustand'
import { devtools } from 'zustand/middleware'

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
  closeLogsModal: () => void
}

type ModalStore = ModalStoreState & ModalStoreActions

const initialState: ModalStoreState = {
  triggerModal: { open: false },
  logsModal: { open: false },
}

export const useModalStore = create<ModalStore>()(
  devtools(
    (set) => ({
      ...initialState,

      openTriggerModal: (pipelineId, rerunOf) =>
        set({
          triggerModal: { open: true, pipelineId, rerunOf },
        }),

      closeTriggerModal: () => set({ triggerModal: { open: false } }),

      openLogsModal: (pipelineId, runNumber) =>
        set({
          logsModal: { open: true, pipelineId, runNumber },
        }),

      closeLogsModal: () => set({ logsModal: { open: false } }),
    }),
    { name: 'ModalStore' }
  )
)
