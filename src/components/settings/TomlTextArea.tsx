import classes from './TomlTextArea.module.css'

interface TomlTextAreaProps {
  value: string
  onChange: (value: string) => void
  disabled?: boolean
}

export function TomlTextArea({ value, onChange, disabled }: TomlTextAreaProps) {
  return (
    <textarea
      className={classes.editor}
      aria-label="config.toml"
      value={value}
      disabled={disabled}
      onChange={(event) => onChange(event.currentTarget.value)}
      autoCapitalize="off"
      autoCorrect="off"
      autoComplete="off"
      spellCheck={false}
      wrap="soft"
    />
  )
}
