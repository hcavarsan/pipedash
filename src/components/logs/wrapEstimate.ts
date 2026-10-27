const TAB_STOP = 8

const isWide = (unit: number): boolean =>
  (unit >= 0x1100 && unit <= 0x115f) ||
  (unit >= 0x2e80 && unit <= 0xa4cf) ||
  (unit >= 0xac00 && unit <= 0xd7a3) ||
  (unit >= 0xf900 && unit <= 0xfaff) ||
  (unit >= 0xfe30 && unit <= 0xfe4f) ||
  (unit >= 0xff00 && unit <= 0xff60) ||
  (unit >= 0xffe0 && unit <= 0xffe6)

export function visualLines(text: string, cols: number): number {
  let width = 0

  for (let index = 0; index < text.length; index += 1) {
    const unit = text.charCodeAt(index)

    if (unit === 9) {
      width += TAB_STOP - (width % TAB_STOP)
    } else {
      width += unit >= 0x1100 && isWide(unit) ? 2 : 1
    }
  }

  return Math.max(1, Math.ceil(width / Math.max(cols, 1)))
}
