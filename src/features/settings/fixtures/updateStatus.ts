import type { UpdateStatus } from '../../../types/updater'

export const updateStatusFixture: UpdateStatus = {
  revision: 1,
  current_version: '0.8.0',
  automatic_updates: false,
  mode: 'native',
  install_source: 'AppImage',
  reason: null,
  target: 'linux-x86_64-appimage',
  phase: 'idle',
  available_version: null,
  release_url: null,
  last_checked_at: null,
  downloaded_bytes: 0,
  total_bytes: null,
  can_check: true,
  can_download: false,
  can_install: false,
  error: null,
  notice: null,
}
