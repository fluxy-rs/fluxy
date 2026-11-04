package com.thep0y.fluxy.mobile

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context

/**
 * The phone's own clipboard.
 *
 * Reading is only allowed while the app is in the foreground (Android 10+), so
 * callers must ask on demand rather than poll. Nothing here is automatic: the
 * settings switch decides whether the shell ever pushes a copy, because a phone
 * clipboard routinely holds passwords and one-time codes.
 */
class ClipboardBridge(private val activity: Activity) {
    private val manager: ClipboardManager?
        get() = activity.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager

    /** Current plain text, or an empty string when the clip is empty. */
    fun read(): String {
        val clip = manager?.primaryClip ?: return ""
        if (clip.itemCount == 0) return ""
        val text = clip.getItemAt(0).coerceToText(activity)?.toString() ?: return ""
        return text
    }

    /** Replace the clipboard with [text]. */
    fun write(text: String) {
        // Logged because the clipboard has no readable surface from adb: without
        // this line a delivery that silently fails looks identical to one that
        // worked, and the only way to tell is to open an app and paste.
        val target = manager
        android.util.Log.i(
            "FluxyClip",
            "write len=" + text.length + " preview=" + text.take(24) + " manager=" + (target != null),
        )
        target?.setPrimaryClip(ClipData.newPlainText("Fluxy", text))
    }
}
