import { create } from 'zustand'
import { devtools } from 'zustand/middleware'

interface TriggerModalState {
  open: boolean
  pipelineId: string | null
  initialInputs?: Record<string, unknown>
}

interface LogsModalState {
  open: boolean
  pipelineId: string | null
  runNumber: number | null
}

interface RerunLoadingState {
  pipelineId: string
  runNumber: number
}

interface ModalStoreState {
  triggerModal: TriggerModalState
  logsModal: LogsModalState
  rerunLoading: RerunLoadingState | null
}

interface ModalStoreActions {
  openTriggerModal: (pipelineId: string, initialInputs?: Record<string, unknown>) => void
  closeTriggerModal: () => void
  openLogsModal: (pipelineId: string, runNumber: number) => void
  closeLogsModal: () => void
  setRerunLoading: (pipelineId: string, runNumber: number) => void
  clearRerunLoading: () => void
}

type ModalStore = ModalStoreState & ModalStoreActions

const initialState: ModalStoreState = {
  triggerModal: {
    open: false,
    pipelineId: null,
    initialInputs: undefined,
  },
  logsModal: {
    open: false,
    pipelineId: null,
    runNumber: null,
  },
  rerunLoading: null,
}

export const useModalStore = create<ModalStore>()(
  devtools(
    (set) => ({
      ...initialState,

      openTriggerModal: (pipelineId, initialInputs) =>
        set({
          triggerModal: { open: true, pipelineId, initialInputs },
        }),

      closeTriggerModal: () =>
        set({
          triggerModal: { open: false, pipelineId: null, initialInputs: undefined },
        }),

      openLogsModal: (pipelineId, runNumber) =>
        set({
          logsModal: { open: true, pipelineId, runNumber },
        }),

      closeLogsModal: () =>
        set({
          logsModal: { open: false, pipelineId: null, runNumber: null },
        }),

      setRerunLoading: (pipelineId, runNumber) =>
        set({
          rerunLoading: { pipelineId, runNumber },
        }),

      clearRerunLoading: () =>
        set({
          rerunLoading: null,
        }),
    }),
    { name: 'ModalStore' }
  )
)
