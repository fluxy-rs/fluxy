package com.thep0y.fluxy.mobile

import android.app.Activity
import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.os.StatFs
import android.provider.DocumentsContract
import org.json.JSONObject
import java.io.File
import java.io.IOException

/**
 * Receive location and export (docs/MOBILE_PLAN.md 4.2).
 *
 * Android 11 forbids writing to a public folder without either all-files access
 * (a Play policy dead end for a file transfer app) or a user grant. So the first
 * launch asks for a directory through the Storage Access Framework, keeps the
 * persistable grant, and creates a `Fluxy` folder inside it: picking Downloads
 * gives Downloads/Fluxy, and every received file is written there as a document
 * the user can open from any file manager.
 *
 * The app still stages an in-progress receive in its private external directory:
 * the transfer pipeline needs random access, checkpoints and resume, which a
 * SAF document stream does not provide. On completion the file is copied into
 * the chosen folder and the staged copy removed.
 */
class ExportBridge(private val activity: Activity) {
    companion object {
        const val FOLDER = "Fluxy"
        const val DIRECTORY_MIME = "vnd.android.document/directory"
        private const val PREFS = "fluxy-mobile"
        private const val TREE_KEY = "receive-tree"
    }

    data class State(val configured: Boolean, val label: String)

    private val prefs = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val resolver: ContentResolver get() = activity.contentResolver

    /** Where an in-progress receive is written. Never shown as the destination. */
    fun stagingDir(): File {
        val base = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) ?: activity.filesDir
        return File(base, FOLDER).apply { mkdirs() }
    }

    /** Free bytes on the staging volume, for the pre-accept check. */
    fun availableBytes(): Long {
        return runCatching {
            Stats(stagingDir()).availableBytes
        }.getOrDefault(0L)
    }

    private class Stats(dir: File) {
        val availableBytes: Long = StatFs(dir.absolutePath).availableBytes
    }

    /**
     * The current destination. A tree whose grant was revoked (the user cleared
     * the app's storage, or the provider is gone) reads as unconfigured rather
     * than failing every export later.
     */
    fun status(): State {
        val uri = storedTree() ?: return State(false, "")
        val granted = resolver.persistedUriPermissions.any { it.uri == uri && it.isWritePermission }
        if (!granted) {
            prefs.edit().remove(TREE_KEY).apply()
            return State(false, "")
        }
        return runCatching { State(true, label(uri)) }.getOrElse { State(false, "") }
    }

    /**
     * Remember the folder the picker returned, make sure the Fluxy folder exists,
     * and answer with the destination. Throws when the folder cannot be used, so
     * the picker reports the failure instead of claiming success.
     */
    fun accept(tree: Uri): State {
        resolver.takePersistableUriPermission(
            tree,
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
        )
        ensureFolder(tree)
        prefs.edit().putString(TREE_KEY, tree.toString()).apply()
        return State(true, label(tree)).also { writeMarker() }
    }

    /**
     * Copy a finished staged file into the chosen folder, creating it there and
     * returning its document URI. The document is deleted again on a failed copy,
     * so a half-written file never appears in the user's file manager.
     */
    fun export(path: String, name: String, mime: String): String {
        val tree = storedTree() ?: throw IOException("choose a receive folder first")
        val folder = ensureFolder(tree)
        val unique = uniqueName(tree, folder, name)
        val document = DocumentsContract.createDocument(
            resolver,
            folder,
            mime.ifBlank { "application/octet-stream" },
            unique,
        ) ?: throw IOException("the receive folder refused a new file")
        try {
            resolver.openOutputStream(document, "w")?.use { output ->
                File(path).inputStream().use { input -> input.copyTo(output, 1 shl 16) }
            } ?: throw IOException("the receive folder refused an output stream")
        } catch (error: Exception) {
            runCatching { DocumentsContract.deleteDocument(resolver, document) }
            throw error
        }
        return document.toString()
    }

    /**
     * Write the destination next to the app data so the Rust setup hook can read
     * it synchronously, before the WebView exists and before it may accept an
     * offer. Called on every launch and after a pick.
     */
    fun writeMarker() {
        val state = status()
        runCatching {
            val marker = JSONObject()
                .put("configured", state.configured)
                .put("label", state.label)
            File(activity.dataDir, "receive-tree.json").writeText(marker.toString(), Charsets.UTF_8)
        }
    }

    private fun storedTree(): Uri? {
        val raw = prefs.getString(TREE_KEY, null) ?: return null
        return runCatching { Uri.parse(raw) }.getOrNull()
    }

    private fun treeRoot(tree: Uri): Uri =
        DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))

    private fun displayName(uri: Uri): String? {
        val columns = arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
        resolver.query(uri, columns, null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) return cursor.getString(0)
        }
        return null
    }

    /**
     * The folder received files land in. Picking Downloads creates
     * Downloads/Fluxy; a folder already called Fluxy is used directly instead of
     * growing a second level.
     */
    private fun ensureFolder(tree: Uri): Uri {
        val root = treeRoot(tree)
        if (displayName(root)?.equals(FOLDER, ignoreCase = true) == true) {
            return root
        }
        findChild(tree, root, FOLDER, DIRECTORY_MIME)?.let { return it }
        return DocumentsContract.createDocument(resolver, root, DIRECTORY_MIME, FOLDER)
            ?: throw IOException("the chosen folder refused the Fluxy subfolder")
    }

    /**
     * The label shown in settings. Derived from the document id rather than the
     * display name alone: a folder already called Fluxy keeps its parent in the
     * label, so `Download/Fluxy` does not collapse to a bare `Fluxy`.
     */
    private fun label(tree: Uri): String {
        val id = DocumentsContract.getTreeDocumentId(tree)
        val path = id.substringAfter(':', id).trim('/')
        val name = displayName(treeRoot(tree)).orEmpty()
        return when {
            path.endsWith("/$FOLDER", ignoreCase = true) -> path
            name.equals(FOLDER, ignoreCase = true) -> if (path.isEmpty()) FOLDER else path
            path.isEmpty() -> FOLDER
            else -> "$path/$FOLDER"
        }
    }

    private fun findChild(tree: Uri, parent: Uri, name: String, mime: String): Uri? {
        val parentId = DocumentsContract.getDocumentId(parent)
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, parentId)
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
        )
        resolver.query(children, columns, null, null, null)?.use { cursor ->
            while (cursor.moveToNext()) {
                val display = cursor.getString(1)
                if (display.equals(name, ignoreCase = true) && cursor.getString(2) == mime) {
                    return DocumentsContract.buildDocumentUriUsingTree(tree, cursor.getString(0))
                }
            }
        }
        return null
    }

    /** Keep a duplicate name from replacing a file that is already there. */
    private fun uniqueName(tree: Uri, folder: Uri, name: String): String {
        val folderId = DocumentsContract.getDocumentId(folder)
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, folderId)
        val taken = mutableSetOf<String>()
        resolver.query(children, arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                while (cursor.moveToNext()) cursor.getString(0)?.lowercase()?.let(taken::add)
            }
        if (name.lowercase() !in taken) return name
        val dot = name.lastIndexOf('.')
        val stem = if (dot > 0) name.substring(0, dot) else name
        val extension = if (dot > 0) name.substring(dot) else ""
        var index = 1
        while (true) {
            val candidate = "$stem ($index)$extension"
            if (candidate.lowercase() !in taken) return candidate
            index += 1
        }
    }
}
