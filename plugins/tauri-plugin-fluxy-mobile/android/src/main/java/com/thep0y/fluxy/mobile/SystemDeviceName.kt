package com.thep0y.fluxy.mobile

import android.content.Context
import android.os.Build
import android.provider.Settings

/**
 * The name this phone goes by.
 *
 * Order matters: a name the user set in the system settings is the one they will
 * recognise, and `Build.MODEL` alone is often an opaque codename. Only when
 * neither exists does the app fall back to its own default.
 */
object SystemDeviceName {
    fun read(context: Context): String {
        userSet(context)?.let { return it }
        val brand = Build.BRAND?.trim()?.replaceFirstChar { it.uppercaseChar() }.orEmpty()
        val model = Build.MODEL?.trim().orEmpty()
        val combined = listOf(brand, model).filter { it.isNotEmpty() }.joinToString(" ")
        return combined.ifEmpty { DEFAULT }
    }

    private fun userSet(context: Context): String? = runCatching {
        @Suppress("DEPRECATION")
        Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)
    }.getOrNull()?.trim()?.takeIf { it.isNotEmpty() }

    const val DEFAULT: String = "Fluxy device"
}
