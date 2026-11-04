package com.thep0y.fluxy.mobile

import android.app.Activity
import android.content.ContentValues
import android.os.Build
import android.os.Environment
import android.os.StatFs
import android.provider.MediaStore
import java.io.File
import java.io.IOException

/**
 * Receive location and export (docs/MOBILE_PLAN.md 4.2).
 *
 * Received files land in the app's external files directory when it is
 * available, so they survive a cache purge and need no storage permission. The
 * export step then copies a finished file into the public Downloads collection
 * through MediaStore with IS_PENDING, which keeps a half-copied entry invisible
 * to other apps.
 */
class ExportBridge(private val activity: Activity) {
    companion object {
        private const val FOLDER = "Fluxy"
    }

    /**
     * Where received files land. With all-files access the public Download/Fluxy
     * folder is used, so anything that arrives is immediately visible in the
     * user's file manager and apps can open and edit it in place. Without the
     * grant the app can only write its private directory, which the user cannot
     * reach on Android 11+, so that stays as the fallback.
     */
    fun receiveDir(): File {
        val public = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), FOLDER)
        if (Environment.isExternalStorageManager() && (public.mkdirs() || public.isDirectory)) {
            return public
        }
        val external = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
        val base = external ?: activity.filesDir
        return File(base, FOLDER).apply { mkdirs() }
    }

    /** True once the user granted the all-files access this app needs. */
    fun hasAllFilesAccess(): Boolean = Environment.isExternalStorageManager()

    /** Free bytes on the receive volume. */
    fun availableBytes(): Long {
        return runCatching {
            val stat = StatFs(receiveDir().absolutePath)
            stat.availableBytes
        }.getOrDefault(0L)
    }

    /**
     * Copy a finished sandbox file into Downloads. Returns the new content URI.
     * The entry stays pending until the copy succeeds, so a failure leaves no
     * half-written download behind.
     */
    fun export(path: String, name: String, mime: String): String {
        val source = File(path)
        if (!source.isFile) throw IOException("the received file is missing")
        val resolver = activity.contentResolver
        val supportsPending = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, name)
            put(MediaStore.Downloads.MIME_TYPE, mime)
            if (supportsPending) put(MediaStore.Downloads.IS_PENDING, 1)
        }
        val collection = if (supportsPending) {
            MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
        } else {
            MediaStore.Downloads.EXTERNAL_CONTENT_URI
        }
        val uri = resolver.insert(collection, values)
            ?: throw IOException("Downloads refused a new entry")
        try {
            resolver.openOutputStream(uri)?.use { output ->
                source.inputStream().use { input -> input.copyTo(output, 1 shl 16) }
            } ?: throw IOException("Downloads refused an output stream")
            if (supportsPending) {
                val publish = ContentValues().apply {
                    put(MediaStore.Downloads.IS_PENDING, 0)
                }
                resolver.update(uri, publish, null, null)
            }
        } catch (error: Exception) {
            runCatching { resolver.delete(uri, null, null) }
            throw error
        }
        return uri.toString()
    }
}
