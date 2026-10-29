export function openRowMenu(row: HTMLElement): boolean {
  const target = row.querySelector<HTMLElement>('[data-row-menu]')

  if (!target || target.matches(':disabled, [aria-disabled="true"]')) {
    return false
  }

  target.click()

  return true
}
