const OPEN_OVERLAY = '.mantine-Modal-content, .mantine-Popover-dropdown, .mantine-Menu-dropdown'

export function overlayOpen(): boolean {
  return Array.from(document.querySelectorAll(OPEN_OVERLAY)).some((element) => element.getClientRects().length > 0)
}

export function dialogOpen(): boolean {
  return Array.from(document.querySelectorAll('.mantine-Modal-content')).some(
    (element) => element.getClientRects().length > 0
  )
}
