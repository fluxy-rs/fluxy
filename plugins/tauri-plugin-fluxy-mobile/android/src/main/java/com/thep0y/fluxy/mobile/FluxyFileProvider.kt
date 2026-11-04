package com.thep0y.fluxy.mobile

import androidx.core.content.FileProvider

/**
 * A FileProvider of Fluxy's own.
 *
 * Tauri ships one with the same class name, and the manifest merger folds two
 * <provider> elements that name the same class into one - whichthen collides on
 * authorities and on the paths resource. Subclassing keeps them apart: this one
 * owns "${applicationId}.fluxy.files" and the receive directory, Tauri keeps
 * its own for asset:// loading.
 */
class FluxyFileProvider : FileProvider()
