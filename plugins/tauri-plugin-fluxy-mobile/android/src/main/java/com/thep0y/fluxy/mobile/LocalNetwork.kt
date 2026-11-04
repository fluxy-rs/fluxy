package com.thep0y.fluxy.mobile

import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log

/**
 * The Android 16 local network permission.
 *
 * From API 36 an app may not touch the LAN, including NsdManager, until the user
 * grants ACCESS_LOCAL_NETWORK; without it registration throws
 * "Missing local network permission" and every socket to a peer is blocked.
 * The prompt is requested once when the webview loads, and discovery keeps
 * retrying so an answer that arrives later still starts it.
 */
class LocalNetwork(private val activity: Activity) {
    /// True when the app may use the LAN.
    ///
    /// The permission is new enough that some API 36 builds do not define it at
    /// all; there, checking would report DENIED forever and discovery would never
    /// start. Ask the package manager whether the permission exists first.
    fun granted(): Boolean {
        if (Build.VERSION.SDK_INT < 36) return true
        val defined = runCatching {
            activity.packageManager.getPermissionInfo(PERMISSION, 0)
        }.isSuccess
        if (!defined) return true
        return activity.checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED
    }

    /// Ask the system. The answer arrives through the activity, not this class,
    /// so discovery retries until the permission is granted rather than waiting
    /// for a callback that would never come.
    fun request() {
        if (Build.VERSION.SDK_INT < 36 || granted()) return
        val defined = runCatching {
            activity.packageManager.getPermissionInfo(PERMISSION, 0)
        }.isSuccess
        if (!defined) return
        runCatching { activity.requestPermissions(arrayOf(PERMISSION), REQUEST_CODE) }
            .onFailure { Log.w(TAG, "permission prompt failed: " + it.message) }
    }

    companion object {
        private const val TAG = "FluxyDiscovery"

        /** Introduced in Android 16 (API 36); earlier releases have no such gate. */
        const val PERMISSION = "android.permission.ACCESS_LOCAL_NETWORK"

        private const val REQUEST_CODE = 4701
    }
}
