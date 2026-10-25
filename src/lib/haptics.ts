import { type HapticKind, mobileService } from '../services/mobile'
import { isMobilePlatform } from '../utils/platform'

export function haptic(kind: HapticKind): void {
  if (!isMobilePlatform()) {
    return
  }

  mobileService.haptic(kind).catch(() => undefined)
}
