package com.thep0y.fluxy.mobile

import android.app.Activity
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.os.Build
import android.view.View

/**
 * Paints the window behind the system bars.
 *
 * The activity content view keeps itself out of the status bar with insets, so
 * whatever sits behind that bar is the window background. Matching it to the page
 * background makes the bar look like part of the app instead of a themed strip,
 * and the icon contrast is set from the same colour.
 */
class Chrome(private val activity: Activity) {
    fun setBackground(color: String) {
        val parsed = runCatching { Color.parseColor(color) }.getOrNull() ?: return
        activity.runOnUiThread {
            activity.window.setBackgroundDrawable(ColorDrawable(parsed))
            activity.window.decorView.setBackgroundColor(parsed)
            val light = luminance(parsed) > 0.5
            val controller = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                activity.window.insetsController
            } else {
                @Suppress("DEPRECATION")
                activity.window.decorView.systemUiVisibility
                null
            }
            if (controller != null) {
                val appearance = if (light) {
                    android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                } else {
                    0
                }
                controller.setSystemBarsAppearance(
                    appearance,
                    android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS,
                )
            } else if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
                @Suppress("DEPRECATION")
                val flags = activity.window.decorView.systemUiVisibility
                @Suppress("DEPRECATION")
                activity.window.decorView.systemUiVisibility = if (light) {
                    flags or View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
                } else {
                    flags and View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR.inv()
                }
            }
        }
    }

    /** Relative luminance, enough to decide dark or light icons. */
    private fun luminance(color: Int): Double {
        fun channel(value: Int): Double {
            val s = value / 255.0
            return if (s <= 0.03928) s / 12.92 else Math.pow((s + 0.055) / 1.055, 2.4)
        }
        val r = channel(Color.red(color))
        val g = channel(Color.green(color))
        val b = channel(Color.blue(color))
        return 0.2126 * r + 0.7152 * g + 0.0722 * b
    }
}
