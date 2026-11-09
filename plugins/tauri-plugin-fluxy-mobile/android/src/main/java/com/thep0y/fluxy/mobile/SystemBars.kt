package com.thep0y.fluxy.mobile

import android.app.Activity
import android.os.Build
import android.view.View
import android.view.WindowInsets
import android.webkit.WebView

/**
 * Keeps the WebView out of the system bars.
 *
 * Android 15 draws app content edge to edge, so without this the page starts
 * under the status bar. The platform way to fix that is to let the view consume
 * the insets and pad itself; the page below then lays out inside a smaller
 * viewport and needs no CSS of its own.
 */
class SystemBars(private val activity: Activity) {
    fun insets(): Map<String, Int> {
        val root = activity.window.decorView.rootWindowInsets
        if (root == null) return mapOf("top" to 0, "bottom" to 0)
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val bars = root.getInsets(WindowInsets.Type.systemBars())
            mapOf("top" to bars.top, "bottom" to bars.bottom)
        } else {
            @Suppress("DEPRECATION")
            mapOf("top" to root.systemWindowInsetTop, "bottom" to root.systemWindowInsetBottom)
        }
    }

    /**
     * Pad the WebView by the current system bar insets and keep doing it as they
     * change (rotation, keyboard, a bar appearing).
     */
    fun applyTo(webView: WebView) {
        // The activity content view owns the whole window, so it is the view that
        // has to consume the bars; padding the WebView alone leaves its layout
        // parent still edge to edge.
        val target = activity.findViewById<View>(android.R.id.content) ?: webView
        target.setOnApplyWindowInsetsListener { view: View, insets: WindowInsets ->
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val bars = insets.getInsets(WindowInsets.Type.systemBars())
                // The keyboard is not part of systemBars(). Without subtracting
                // it the page keeps its full height and the field being edited
                // stays underneath the keyboard.
                val ime = insets.getInsets(WindowInsets.Type.ime())
                view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
            } else {
                @Suppress("DEPRECATION")
                view.setPadding(
                    insets.systemWindowInsetLeft,
                    insets.systemWindowInsetTop,
                    insets.systemWindowInsetRight,
                    insets.systemWindowInsetBottom,
                )
            }
            insets
        }
        target.requestApplyInsets()
    }
}
