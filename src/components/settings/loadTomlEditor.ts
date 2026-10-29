import type { TomlEditor } from './TomlEditor'

export async function loadTomlEditor(): Promise<{ default: typeof TomlEditor }> {
  const [{ loader }, monaco, editorModule] = await Promise.all([
    import('@monaco-editor/react'),
    import('monaco-editor/editor/editor.api'),
    import('./TomlEditor'),
  ])

  loader.config({ monaco })

  return { default: editorModule.TomlEditor }
}
