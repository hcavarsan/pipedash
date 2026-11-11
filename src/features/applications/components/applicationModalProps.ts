import type { ApplicationCopy, ApplicationDetail } from '../../../types/applications'

export interface ApplicationModalProps {
  detail: ApplicationDetail
  copy: ApplicationCopy
  onClose: () => void
}
