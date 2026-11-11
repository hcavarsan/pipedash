export const TABLE_HEADER_STYLE = {
  fontSize: 'var(--mantine-font-size-sm)',
  fontWeight: 600,
  paddingTop: 'var(--mantine-spacing-sm)',
  paddingBottom: 'var(--mantine-spacing-sm)',
  paddingLeft: 'var(--mantine-spacing-md)',
  paddingRight: 'var(--mantine-spacing-md)',
  whiteSpace: 'nowrap',
  verticalAlign: 'middle',
  borderBottom: '1px solid var(--mantine-color-default-border)',
} as const

export const TABLE_NUMERIC = { fontVariantNumeric: 'tabular-nums' } as const

export const rowClassName = (clickable: boolean): string | undefined =>
  clickable ? 'interactive-surface row-clickable' : undefined
