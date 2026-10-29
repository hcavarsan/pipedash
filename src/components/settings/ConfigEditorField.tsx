import { px, useComputedColorScheme, useMantineTheme } from '@mantine/core'
import { lazy, type ReactNode, Suspense } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { SPACING_PX } from '../../theme'
import { LoadingState } from '../common/LoadingState'
import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary'

import { loadTomlEditor } from './loadTomlEditor'
import { TomlTextArea } from './TomlTextArea'

const TomlEditor = lazy(loadTomlEditor)

interface ConfigEditorFieldProps {
  value: string
  disabled: boolean
  onChange: (value: string) => void
}

export function ConfigEditorField({ value, disabled, onChange }: ConfigEditorFieldProps): ReactNode {
  const colorScheme = useComputedColorScheme('dark')
  const theme = useMantineTheme()
  const mobileShell = useMobileShell()

  return (
    <ErrorBoundary scope="feature" name="Configuration editor">
      {mobileShell ? (
        <TomlTextArea value={value} onChange={onChange} disabled={disabled} />
      ) : (
        <Suspense fallback={<LoadingState message="Loading configuration editor…" />}>
          <TomlEditor
            height="100%"
            theme={colorScheme === 'dark' ? 'vs-dark' : 'light'}
            value={value}
            onChange={(next) => onChange(next ?? '')}
            options={{
              readOnly: disabled,
              fontSize: Number(px(theme.fontSizes.sm)),
              ariaLabel: 'Configuration editor',
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              wordWrap: 'on',
              automaticLayout: true,
              tabSize: 2,
              insertSpaces: true,
              lineNumbers: 'on',
              glyphMargin: false,
              folding: true,
              lineDecorationsWidth: 0,
              lineNumbersMinChars: 3,
              padding: { top: SPACING_PX.sm, bottom: SPACING_PX.sm },
            }}
          />
        </Suspense>
      )}
    </ErrorBoundary>
  )
}
