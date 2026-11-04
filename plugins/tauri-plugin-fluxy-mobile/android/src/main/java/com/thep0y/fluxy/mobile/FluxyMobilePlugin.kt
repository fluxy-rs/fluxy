package com.thep0y.fluxy.mobile

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.os.Environment
import android.webkit.WebView
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.net.Uri
import androidx.core.app.NotificationCompat
import java.io.File

@InvokeArg
class NotifyChatArgs {
    var title: String = ""
    var body: String = ""
    var deviceId: String = ""
}

@InvokeArg
class ClipboardTextArgs {
    var text: String = ""
}

@InvokeArg
class DiscoveryStartArgs {
    var port: Int = 0
    var deviceId: String = ""
    var name: String = ""
    var fingerprint: String = ""
}

@InvokeArg
class ChromeColorArgs {
    lateinit var color: String
}

@InvokeArg
class FsExportArgs {
    lateinit var path: String
    lateinit var name: String
    lateinit var mime: String
}

@InvokeArg
class SecretSetArgs {
    lateinit var value: String
}

private const val CHAT_CHANNEL_ID = "fluxy-chat"
private const val CHAT_NOTIFICATION_ID = 4201
private const val CHAT_REQUEST_CODE = 4202

@TauriPlugin
class FluxyMobilePlugin(private val host: Activity) : Plugin(host) {
    private val localNetwork by lazy { LocalNetwork(host) }
    private val clipboard by lazy { ClipboardBridge(host) }
    private val sources by lazy { SourceBridge(host) }
    private val exports by lazy { ExportBridge(host) }
    private val bars by lazy { SystemBars(host) }
    private val chrome by lazy { Chrome(host) }
    private val network by lazy { NetworkInfo(host) }
    private val discovery by lazy { Discovery(host) }

    /** Pending picker call, resolved from the registry callback. */
    private var pendingPick: Invoke? = null

    private val pickLauncher = try {
        (host as androidx.activity.ComponentActivity).activityResultRegistry.register(
            "fluxy-pick-source",
            androidx.activity.result.contract.ActivityResultContracts.OpenDocument(),
        ) { uri: android.net.Uri? ->
            val invoke = pendingPick
            pendingPick = null
            android.util.Log.i("FluxyPlugin", "picker returned uri=" + uri + " pending=" + (invoke != null))
            if (invoke == null) return@register
            if (uri == null) {
                val result = JSObject()
                result.put("cancelled", true)
                invoke.resolve(result)
                return@register
            }
            try {
                sources.takePersistableRead(uri)
                val opened = sources.open(uri)
                val result = JSObject()
                result.put("cancelled", false)
                result.put("fd", (opened["fd"] as Int).toLong())
                result.put("uri", opened["uri"] as String)
                result.put("name", opened["name"] as String)
                result.put("size", opened["size"] as Long)
                result.put("mime", opened["mime"] as String)
                invoke.resolve(result)
            } catch (error: Exception) {
                android.util.Log.w("FluxyPlugin", "opening the picked document failed: " + error.message)
                invoke.reject(error.message ?: "the document could not be opened")
            }
        }
    } catch (error: Exception) {
        null
    }

    private val secrets: KeystoreSecrets by lazy {
        KeystoreSecrets(File(host.dataDir, "fluxy-identity.bin"))
    }

    init {
        // Ask for all-files access once, so received files can land in the
        // public Download/Fluxy folder instead of the private directory the
        // user cannot open. The system shows its own consent screen; the marker
        // keeps a declined user from being redirected on every launch.
        runCatching {
            val marker = File(host.dataDir, "storage-access-asked.txt")
            if (!Environment.isExternalStorageManager() && !marker.exists()) {
                marker.writeText("1", Charsets.UTF_8)
                host.startActivity(
                    Intent(android.provider.Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION).apply {
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    },
                )
            }
        }
        // Rust loads the identity synchronously in the setup hook, so the
        // sealed copy has to become a plain file before that runs.
        runCatching {
            secrets.materializeIdentity(File(host.dataDir, "identity.json"))
        }
        // Rust also needs the receive directory before it can accept a file.
        // A marker file keeps that a plain synchronous read on its side.
        runCatching {
            File(host.dataDir, "receive-dir.txt")
                .writeText(exports.receiveDir().absolutePath, Charsets.UTF_8)
        }
    }

    override fun load(webView: WebView) {
        super.load(webView)
        // Android 15 draws edge to edge; pad the view by the system bars so the
        // page lays out below the status bar without any CSS of its own.
        runCatching { bars.applyTo(webView) }
        // Android 16 blocks the LAN until this is granted; ask while the user is
        // looking at the app rather than at the first failed pairing.
        runCatching { LocalNetwork(host).request() }
    }

    @Command
    fun secretGet(invoke: Invoke) {
        try {
            val value = secrets.read()
            val result = JSObject()
            result.put("value", value)
            invoke.resolve(result)
        } catch (error: Exception) {
            invoke.reject(error.message ?: "the secret could not be read")
        }
    }

    @Command
    fun secretSet(invoke: Invoke) {
        val args = invoke.parseArgs(SecretSetArgs::class.java)
        try {
            secrets.write(args.value)
            invoke.resolve()
        } catch (error: Exception) {
            invoke.reject(error.message ?: "the secret could not be written")
        }
    }

    /** Default device name: what the user named the phone, else brand + model. */
    @Command
    fun deviceName(invoke: Invoke) {
        val result = JSObject()
        result.put("name", SystemDeviceName.read(host))
        invoke.resolve(result)
    }

    /** Whether the app may use the LAN; the page shows a banner when not. */
    @Command
    fun localNetworkGranted(invoke: Invoke) {
        val result = JSObject()
        result.put("granted", localNetwork.granted())
        invoke.resolve(result)
    }

    /** Read the phone clipboard; only meaningful while the app is in front. */
    @Command
    fun clipboardRead(invoke: Invoke) {
        runCatching {
            val result = JSObject()
            result.put("text", clipboard.read())
            invoke.resolve(result)
        }.onFailure {
            invoke.reject(it.message ?: "the clipboard could not be read")
        }
    }

    /** Replace the phone clipboard. */
    @Command
    fun clipboardWrite(invoke: Invoke) {
        val args = invoke.parseArgs(ClipboardTextArgs::class.java)
        runCatching { clipboard.write(args.text) }
            .onSuccess { invoke.resolve() }
            .onFailure { invoke.reject(it.message ?: "the clipboard could not be written") }
    }

    /**
     * Post a chat notification.
     *
     * Done here rather than through the notification plugin because the tap has
     * to reach the message list: the PendingIntent below carries a fluxy://chat
     * deep link, which the activity already routes into PendingShare.
     */
    @Command
    fun notifyChat(invoke: Invoke) {
        val args = invoke.parseArgs(NotifyChatArgs::class.java)
        try {
            val manager = host.getSystemService(Activity.NOTIFICATION_SERVICE) as NotificationManager
            ensureChatChannel(manager)

            // The plugin library cannot reference the app module's activity,
            // so resolve it by name; the package is the app id.
            val mainClass = Class.forName("${host.packageName}.MainActivity")
            val open = Intent(host, mainClass).apply {
                action = Intent.ACTION_VIEW
                data = Uri.parse("fluxy://chat?device=" + Uri.encode(args.deviceId))
                flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            }
            val pending = PendingIntent.getActivity(
                host,
                CHAT_REQUEST_CODE,
                open,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
            val notification = NotificationCompat.Builder(host, CHAT_CHANNEL_ID)
                .setContentTitle(args.title)
                .setContentText(args.body)
                .setSmallIcon(android.R.drawable.ic_dialog_email)
                .setAutoCancel(true)
                .setContentIntent(pending)
                .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                .build()
            manager.notify(CHAT_NOTIFICATION_ID, notification)
            invoke.resolve()
        } catch (error: Throwable) {
            invoke.reject(error.message ?: "could not post the notification")
        }
    }

    /** True once if the app was opened by tapping a chat notification. */
    @Command
    fun openChat(invoke: Invoke) {
        // Null device means "opened by a notification that named none"; the
        // page falls back to the message list in that case.
        val result = JSObject()
        result.put("device", PendingShare.takeOpenChat())
        invoke.resolve(result)
    }

    private fun ensureChatChannel(manager: NotificationManager) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        if (manager.getNotificationChannel(CHAT_CHANNEL_ID) != null) return
        val channel = NotificationChannel(
            CHAT_CHANNEL_ID,
            "Messages",
            NotificationManager.IMPORTANCE_DEFAULT,
        )
        channel.description = "New messages from paired devices"
        manager.createNotificationChannel(channel)
    }

    /** File URIs shared into the app since the last call; the page drains them. */
    @Command
    fun sharedFiles(invoke: Invoke) {
        val uris = PendingShare.take()
        val target = PendingShare.takeTarget()
        android.util.Log.i("FluxyPlugin", "sharedFiles drained=" + uris.size + " target=" + target)
        val array = JSArray()
        for (uri in uris) {
            val entry = JSObject()
            entry.put("uri", uri.toString())
            array.put(entry)
        }
        val result = JSObject()
        result.put("files", array)
        target?.let { result.put("device", it) }
        invoke.resolve(result)
    }

    /** Keep the process alive while a transfer runs. */
    @Command
    fun foregroundStart(invoke: Invoke) {
        // Android 13+ refuses startForeground without the notification grant,
        // so ask for it the first time a transfer needs the service.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            host.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) !=
            android.content.pm.PackageManager.PERMISSION_GRANTED
        ) {
            runCatching {
                host.requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 0x51)
            }
        }
        runCatching { TransferService.start(host) }
            .onSuccess { invoke.resolve() }
            .onFailure { invoke.reject(it.message ?: "the transfer service could not start") }
    }

    /** Release the foreground slot once nothing is moving. */
    @Command
    fun foregroundStop(invoke: Invoke) {
        runCatching { TransferService.stop(host) }
            .onSuccess { invoke.resolve() }
            .onFailure { invoke.reject(it.message ?: "the transfer service could not stop") }
    }

    /** Open the system document picker and hand back an open descriptor. */
    @Command
    fun fsPickSource(invoke: Invoke) {
        val launcher = pickLauncher
        if (launcher == null) {
            invoke.reject("the document picker is not available in this activity")
            return
        }
        if (pendingPick != null) {
            invoke.reject("a document picker is already open")
            return
        }
        android.util.Log.i("FluxyPlugin", "launching the document picker")
        pendingPick = invoke
        try {
            launcher.launch(arrayOf("*/*"))
        } catch (error: Exception) {
            pendingPick = null
            invoke.reject(error.message ?: "the document picker could not be opened")
        }
    }

    /** Reopen a URI that survived a restart, for resuming a transfer. */
    @Command
    fun fsOpenSource(invoke: Invoke) {
        val args = invoke.parseArgs(SourceUriArgs::class.java)
        try {
            val uri = android.net.Uri.parse(args.uri)
            sources.takePersistableRead(uri)
            val opened = sources.open(uri)
            val result = JSObject()
            result.put("fd", (opened["fd"] as Int).toLong())
            result.put("uri", opened["uri"] as String)
            result.put("name", opened["name"] as String)
            result.put("size", opened["size"] as Long)
            result.put("mime", opened["mime"] as String)
            invoke.resolve(result)
        } catch (error: Exception) {
            invoke.reject(error.message ?: "the document could not be reopened")
        }
    }

    /** Copy a finished sandbox file into the public Downloads collection. */
    @Command
    fun fsExport(invoke: Invoke) {
        val args = invoke.parseArgs(FsExportArgs::class.java)
        try {
            val uri = exports.export(args.path, args.name, args.mime)
            val result = JSObject()
            result.put("uri", uri)
            invoke.resolve(result)
        } catch (error: Exception) {
            invoke.reject(error.message ?: "the file could not be exported")
        }
    }

    /** Register our service and browse for peers through NsdManager. */
    @Command
    fun discoveryStart(invoke: Invoke) {
        val args = invoke.parseArgs(DiscoveryStartArgs::class.java)
        val started = discovery.start(
            args.port,
            args.deviceId,
            args.name,
            args.fingerprint,
        )
        val result = JSObject()
        result.put("started", started)
        invoke.resolve(result)
    }

    @Command
    fun discoveryStop(invoke: Invoke) {
        discovery.stop()
        invoke.resolve()
    }

    /** Peers NsdManager has resolved so far. */
    @Command
    fun discoveryPeers(invoke: Invoke) {
        val result = JSObject()
        val list = org.json.JSONArray()
        for (peer in discovery.snapshot()) {
            val entry = JSObject()
            for ((key, value) in peer) {
                entry.put(key, value)
            }
            list.put(entry)
        }
        result.put("peers", list)
        invoke.resolve(result)
    }

    /** Active network facts: gateway, interface, transport. */
    @Command
    fun localNetworkInfo(invoke: Invoke) {
        val info = network.describe()
        val result = JSObject()
        result.put("gateway", info["gateway"])
        result.put("interface", info["interface"])
        result.put("wifi", info["wifi"])
        result.put("hotspot", info["hotspot"])
        // The listener needs these to bind a real address instead of 0.0.0.0;
        // describe() has always produced them, this command just dropped them.
        // A Kotlin List serialises to a string, so build a real JSArray.
        val addresses = JSArray()
        (info["addresses"] as? List<*>)?.forEach { addresses.put(it as? String ?: "") }
        result.put("addresses", addresses)
        invoke.resolve(result)
    }

    /** Paint the window behind the status bar to match the page background. */
    @Command
    fun setChromeColor(invoke: Invoke) {
        val args = invoke.parseArgs(ChromeColorArgs::class.java)
        try {
            chrome.setBackground(args.color)
            invoke.resolve()
        } catch (error: Exception) {
            invoke.reject(error.message ?: "the chrome colour was refused")
        }
    }

    /** System bar insets in pixels, so the page can pad itself. */
    @Command
    fun systemInsets(invoke: Invoke) {
        val values = bars.insets()
        val result = JSObject()
        result.put("top", values["top"] ?: 0)
        result.put("bottom", values["bottom"] ?: 0)
        invoke.resolve(result)
    }

    /** Free bytes on the receive volume, for the pre-accept check. */
    @Command
    fun storageAvailable(invoke: Invoke) {
        val result = JSObject()
        result.put("freeBytes", exports.availableBytes())
        invoke.resolve(result)
    }

    @Command
    fun keystoreAvailable(invoke: Invoke) {
        val result = JSObject()
        result.put("available", secrets.available())
        invoke.resolve(result)
    }
}
