package com.henrique.pipedash.plugin

import android.app.ActivityManager
import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.os.Build
import android.os.PowerManager
import androidx.core.app.NotificationManagerCompat
import app.tauri.plugin.JSObject
import org.json.JSONObject

object RefreshScheduler {
    private const val JOB_ID = 41027
    private const val PERIOD_MS = 15 * 60 * 1000L
    private const val ENABLED_KEY = "notifications.enabled"
    private const val PREFS = "pipedash.native"
    private const val NOTIFICATIONS_OFF = "Allow notifications for Pipedash in Android Settings to get alerts."
    private const val BACKGROUND_RESTRICTED =
        "Background use is restricted for Pipedash. Allow it in Android Settings > Apps > Pipedash > Battery."
    private const val POWER_SAVE = "Battery Saver is on, so background checks can be delayed."
    private const val SCHEDULE_FAILED =
        "Android couldn't schedule background checks. Turn notifications off and on again."

    private var scheduleError: String? = null

    fun notificationsAllowed(context: Context): Boolean =
        NotificationManagerCompat.from(context).areNotificationsEnabled()

    private fun enabled(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(ENABLED_KEY, false)

    private fun scheduler(context: Context): JobScheduler = context.getSystemService(JobScheduler::class.java)

    private fun schedule(context: Context) {
        val job = JobInfo.Builder(JOB_ID, ComponentName(context, NotificationRefreshJob::class.java))
            .setPeriodic(PERIOD_MS)
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
            .setPersisted(true)
            .build()
        scheduleError = if (scheduler(context).schedule(job) == JobScheduler.RESULT_SUCCESS) null else SCHEDULE_FAILED
    }

    private fun cancel(context: Context) {
        scheduler(context).cancel(JOB_ID)
    }

    fun reconcile(context: Context) {
        if (enabled(context) && scheduler(context).getPendingJob(JOB_ID) == null) {
            schedule(context)
        }
    }

    fun settings(context: Context): JSObject {
        val enabled = enabled(context)
        val blocker = when {
            !notificationsAllowed(context) -> NOTIFICATIONS_OFF
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.P &&
                context.getSystemService(ActivityManager::class.java).isBackgroundRestricted -> BACKGROUND_RESTRICTED
            else -> scheduleError
        }
        val advisory =
            if (context.getSystemService(PowerManager::class.java).isPowerSaveMode) POWER_SAVE else null
        val reason = blocker ?: if (enabled) advisory else null
        return JSObject()
            .put("enabled", enabled)
            .put("available", reason == null)
            .put("reason", reason ?: JSONObject.NULL) as JSObject
    }

    fun configure(context: Context, enabled: Boolean): JSObject {
        if (!enabled) {
            save(context, false)
            cancel(context)
            scheduleError = null
            return settings(context)
        }
        if (!notificationsAllowed(context)) {
            throw IllegalStateException("Allow notifications for Pipedash in Settings to turn on alerts.")
        }
        save(context, true)
        schedule(context)
        return settings(context)
    }

    private fun save(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(ENABLED_KEY, value).apply()
    }
}
