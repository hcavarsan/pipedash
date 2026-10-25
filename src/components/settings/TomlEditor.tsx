import Editor, { type EditorProps, loader } from '@monaco-editor/react'
import * as monaco from 'monaco-editor/editor/editor.api'
import 'monaco-editor/features/register.all'
import 'monaco-editor/languages/definitions/ini/register'
import EditorWorker from 'monaco-editor/editor/editor.worker?worker'

window.MonacoEnvironment = { getWorker: () => new EditorWorker() }
loader.config({ monaco })

export default function TomlEditor(props: EditorProps) {
  return <Editor language="ini" {...props} />
}
