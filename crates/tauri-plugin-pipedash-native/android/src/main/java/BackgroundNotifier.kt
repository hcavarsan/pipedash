package com.henrique.pipedash.plugin

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import org.json.JSONArray
import org.json.JSONObject

object BackgroundNotifier {
    const val EXTRA = "com.henrique.pipedash.notification"
    private const val CHANNEL_ID = "default"

    fun post(context: Context, notices: JSONArray) {
        val manager = NotificationManagerCompat.from(context)
        if (notices.length() == 0 || !manager.areNotificationsEnabled()) {
            return
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(CHANNEL_ID, "Default", NotificationManager.IMPORTANCE_DEFAULT)
            context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName) ?: return
        for (index in 0 until notices.length()) {
            val notice = notices.getJSONObject(index)
            val key = notice.getString("key")
            val id = key.hashCode()
            val target = JSONObject()
                .put("key", key)
                .put("pipelineId", field(notice, "pipeline_id"))
                .put("providerId", field(notice, "provider_id"))
            val intent = Intent(launch)
                .putExtra(EXTRA, target.toString())
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            val content = PendingIntent.getActivity(
                context,
                id,
                intent,
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            val notification = NotificationCompat.Builder(context, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle(notice.getString("title"))
                .setContentText(notice.getString("body"))
                .setContentIntent(content)
                .setAutoCancel(true)
                .build()
            try {
                manager.notify(id, notification)
            } catch (_: SecurityException) {
            }
        }
    }

    private fun field(notice: JSONObject, name: String): String? =
        if (notice.isNull(name)) null else notice.getString(name)
}
