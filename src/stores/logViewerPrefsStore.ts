import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

interface LogViewerPrefsState {
  wrap: boolean
  timestamps: boolean
}

interface LogViewerPrefsActions {
  setWrap: (wrap: boolean) => void
  setTimestamps: (timestamps: boolean) => void
}

type LogViewerPrefs = LogViewerPrefsState & LogViewerPrefsActions

const initialState: LogViewerPrefsState = {
  wrap: true,
  timestamps: false,
}

export const useLogViewerPrefs = create<LogViewerPrefs>()(
  devtools(
    persist(
      (set) => ({
        ...initialState,

        setWrap: (wrap) => set({ wrap }),
        setTimestamps: (timestamps) => set({ timestamps }),
      }),
      {
        name: 'pipedash-log-viewer-prefs',
        partialize: (state) => ({
          wrap: state.wrap,
          timestamps: state.timestamps,
        }),
      }
    ),
    { name: 'LogViewerPrefsStore' }
  )
)
