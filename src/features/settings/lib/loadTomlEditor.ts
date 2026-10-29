import type { TomlEditor } from '../components/TomlEditor'

export async function loadTomlEditor(): Promise<{ default: typeof TomlEditor }> {
  const [{ loader }, monaco, editorModule] = await Promise.all([
    import('@monaco-editor/react'),
    import('monaco-editor/editor/editor.api'),
    import('../components/TomlEditor'),
  ])

  loader.config({ monaco })

  return { default: editorModule.TomlEditor }
}
