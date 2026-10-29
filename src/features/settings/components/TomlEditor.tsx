import { Editor, type EditorProps } from '@monaco-editor/react'
import type { ReactNode } from 'react'
import 'monaco-editor/features/register.all'
import 'monaco-editor/languages/definitions/ini/register'
import EditorWorker from 'monaco-editor/editor/editor.worker?worker'

window.MonacoEnvironment = { getWorker: (): Worker => new EditorWorker() }

export function TomlEditor(props: EditorProps): ReactNode {
  return <Editor language="ini" {...props} />
}
