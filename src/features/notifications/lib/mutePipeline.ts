import { useNotificationStore } from '../../../stores/notificationStore'
import { displaySuccessNotification } from '../../../utils/errorDisplay'

export function mutePipeline(pipeline: { id: string; name: string }, muted: boolean): void {
  useNotificationStore.getState().setPipelineMuted(pipeline.id, muted)
  displaySuccessNotification(
    muted
      ? `You won't get notifications about ${pipeline.name}. Unmute it from its menu or in notification settings.`
      : `You'll get notifications about ${pipeline.name} again.`,
    muted ? 'Notifications muted' : 'Notifications unmuted'
  )
}
