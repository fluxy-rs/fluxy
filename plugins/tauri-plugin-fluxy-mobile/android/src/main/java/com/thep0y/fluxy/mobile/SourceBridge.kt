package com.thep0y.fluxy.mobile

import android.app.Activity
import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import java.io.IOException

@InvokeArg
class SourceUriArgs {
    lateinit var uri: String
}

/**
 * SAF bridge for the send side (docs/MOBILE_PLAN.md 4.1).
 *
 * The Rust side never sees a path: a picked document is opened here, the
 * descriptor is detached, and the integer fd plus the display metadata crosses
 * the boundary. Ownership moves to Rust on a successful detach, so the error
 * paths close the descriptor themselves.
 */
class SourceBridge(private val activity: Activity) {
    companion object {
        const val PICK_REQUEST = 0x4658
    }

    /** Open a picked document and hand its descriptor to the caller. */
    fun open(uri: Uri): Map<String, Any> {
        val resolver = activity.contentResolver
        val mime = resolver.getType(uri)
        var name = ""
        var size = -1L
        resolver.query(uri, null, null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (nameIndex >= 0 && !cursor.isNull(nameIndex)) {
                    name = cursor.getString(nameIndex)
                }
                val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
                if (sizeIndex >= 0 && !cursor.isNull(sizeIndex)) {
                    size = cursor.getLong(sizeIndex)
                }
            }
        }
        val descriptor = resolver.openFileDescriptor(uri, "r")
            ?: throw IOException("the document could not be opened")
        val statSize = descriptor.statSize
        val fd = try {
            // Ownership moves to Rust here; from now on closing is its job.
            descriptor.detachFd()
        } catch (error: Exception) {
            descriptor.close()
            throw error
        }
        val result = linkedMapOf<String, Any>(
            // Rust deserializes this into OpenedSource, whose cancelled flag is
            // mandatory; opening an existing URI never means "dismissed".
            "cancelled" to false,
            "fd" to fd,
            "uri" to uri.toString(),
            "name" to name,
            "size" to (if (size >= 0) size else statSize),
            "mime" to (mime ?: "application/octet-stream"),
        )
        return result
    }

    /** Persist the read grant so a resume after a restart can reopen it. */
    fun takePersistableRead(uri: Uri) {
        runCatching {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION,
            )
        }
    }
}

@InvokeArg
class FsOpenSourceArgs {
    lateinit var uri: String
}
