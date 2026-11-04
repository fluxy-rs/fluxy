package com.thep0y.fluxy.mobile

import android.content.Intent
import android.net.Uri

/**
 * Drained by the plugin whenever the WebView asks what was shared into the app.
 *
 * The activity receives ACTION_SEND intents and leaves the URIs here; the send
 * page then offers them. Kept as a process-wide object because the activity is
 * recreated freely while the runtime lives on.
 */
object PendingShare {
    private val lock = Any()
    private val pending = mutableListOf<Uri>()
    /// Device a deep link asked for; when set the page sends without a tap.
    private var target: String? = null

    /** Device id a tapped notification belongs to, if it named one. */
    private var openChat: String? = null

    /**
     * fluxy://send?file=<uri>&device=<id>, used by automation and links, and
     * fluxy://chat from a notification tap.
     *
     * A tapped notification cannot use the Tauri notification plugin's action
     * event: that fires from onNewIntent, which does not reach plugin code on
     * this build. The PendingIntent carries a plain deep link instead, and the
     * activity already routes those here.
     */
    fun captureLink(data: Uri?): Boolean {
        if (data != null && data.scheme == "fluxy" && data.host == "chat") {
            synchronized(lock) { openChat = data.getQueryParameter("device") }
            return true
        }
        if (data == null || data.scheme != "fluxy" || data.host != "send") return false
        val file = data.getQueryParameter("file")?.let { Uri.parse(it) } ?: return false
        synchronized(lock) {
            if (!pending.contains(file)) pending.add(file)
            target = data.getQueryParameter("device")
        }
        return true
    }

    /** Device the pending share should be sent to, if a link named one. */
    fun takeTarget(): String? = synchronized(lock) {
        val out = target
        target = null
        out
    }

    fun capture(intent: Intent?) {
        if (intent == null) return
        val action = intent.action ?: return
        if (action != Intent.ACTION_SEND && action != Intent.ACTION_SEND_MULTIPLE) return
        val incoming = when {
            action == Intent.ACTION_SEND -> {
                @Suppress("DEPRECATION")
                listOfNotNull(intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM))
            }
            else -> {
                @Suppress("DEPRECATION")
                intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM) ?: emptyList()
            }
        }
        if (incoming.isEmpty()) return
        synchronized(lock) {
            // onCreate, onNewIntent and onResume can all see the same intent, so
            // add only what is not already waiting; otherwise one share shows up
            // as two identical files in the send list.
            for (uri in incoming) {
                if (!pending.contains(uri)) pending.add(uri)
            }
        }
    }

    /**
     * Called by the activity when a launch intent carries the notification
     * plugin's id extra. The tap itself is a plain launch as far as the plugin
     * is concerned, so the activity is the only place that can tell.
     */
    fun requestOpenChat(deviceId: String?) {
        synchronized(lock) { openChat = deviceId }
    }

    /** The device a notification asked to open, once. */
    fun takeOpenChat(): String? = synchronized(lock) {
        val out = openChat
        openChat = null
        out
    }

    /** Take everything shared so far; the caller owns it afterwards. */
    fun take(): List<Uri> = synchronized(lock) {
        val out = pending.toList()
        pending.clear()
        out
    }
}
