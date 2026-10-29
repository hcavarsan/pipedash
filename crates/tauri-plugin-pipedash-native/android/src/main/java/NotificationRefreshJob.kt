package com.henrique.pipedash.plugin

import android.app.job.JobParameters
import android.app.job.JobService
import android.util.Log
import org.json.JSONArray

class NotificationRefreshJob : JobService() {
    override fun onStartJob(params: JobParameters): Boolean {
        val context = applicationContext
        Thread {
            try {
                val json = RefreshBridge.runRefresh(context, context.dataDir.absolutePath)
                BackgroundNotifier.post(context, JSONArray(json))
            } catch (e: Throwable) {
                Log.w("PipedashRefresh", "Background refresh failed", e)
            } finally {
                jobFinished(params, false)
            }
        }.start()
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        try {
            RefreshBridge.cancelRefresh()
        } catch (e: Throwable) {
            Log.w("PipedashRefresh", "Couldn't cancel background refresh", e)
        }
        return false
    }
}
