// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

package app.tauri.barcodescanner

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Context.MODE_PRIVATE
import android.content.Intent
import android.content.SharedPreferences
import android.content.pm.PackageManager
import android.graphics.Color
import android.util.Log
import android.graphics.drawable.Drawable
import android.net.Uri
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.provider.Settings
import android.util.Size
import android.view.ViewGroup
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.ActivityResult
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleOwner
import app.tauri.Logger
import app.tauri.PermissionState
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.common.util.concurrent.ListenableFuture
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import java.util.Collections
import java.util.concurrent.ExecutionException

private const val PERMISSION_ALIAS_CAMERA = "camera"
private const val PERMISSION_NAME = Manifest.permission.CAMERA

@InvokeArg
class ScanOptions {
    var formats: Array<String>? = null
    var windowed: Boolean = false
    var cameraDirection: String? = null
}

@TauriPlugin(
    permissions = [
        Permission(strings = [Manifest.permission.CAMERA], alias = "camera")
    ]
)
class BarcodeScannerPlugin(private val activity: Activity) : Plugin(activity),
    ImageAnalysis.Analyzer {
    private lateinit var webView: WebView
    private var previewView: PreviewView? = null
    private var cameraProviderFuture: ListenableFuture<ProcessCameraProvider>? = null
    private var cameraProvider: ProcessCameraProvider? = null
    private var graphicOverlay: GraphicOverlay? = null
    private var camera: Camera? = null
        set(value) {
            field = value
            activeCamera = value
        }
    private var vibrator: Vibrator? = null

    private var scannerOptions: BarcodeScannerOptions? = null
    private var scanner: com.google.mlkit.vision.barcode.BarcodeScanner? = null

    private var requestPermissionResponse: JSObject? = null
    private var windowed = false

    // declare a map constant for allowed barcode formats
    private val supportedFormats = supportedFormats()

    private var savedInvoke: Invoke? = null
    private var webViewBackground: Drawable? = null

    // While the camera is up, Back is the platform's cancel gesture. Without
    // this the preview had no way out: the WebView behind it does not own the
    // camera view, and the activity's default back handling does not know a
    // scan is running, so the key was swallowed and the camera stayed on.
    private var backCallback: OnBackPressedCallback? = null

    override fun load(webView: WebView) {
        super.load(webView)
        this.webView = webView
    }

    private fun supportedFormats(): Map<String, Int> {
        val map: MutableMap<String, Int> = HashMap()
        map["UPC_A"] = Barcode.FORMAT_UPC_A
        map["UPC_E"] = Barcode.FORMAT_UPC_E
        map["EAN_8"] = Barcode.FORMAT_EAN_8
        map["EAN_13"] = Barcode.FORMAT_EAN_13
        map["CODE_39"] = Barcode.FORMAT_CODE_39
        map["CODE_93"] = Barcode.FORMAT_CODE_93
        map["CODE_128"] = Barcode.FORMAT_CODE_128
        map["CODABAR"] = Barcode.FORMAT_CODABAR
        map["ITF"] = Barcode.FORMAT_ITF
        map["AZTEC"] = Barcode.FORMAT_AZTEC
        map["DATA_MATRIX"] = Barcode.FORMAT_DATA_MATRIX
        map["PDF_417"] = Barcode.FORMAT_PDF417
        map["QR_CODE"] = Barcode.FORMAT_QR_CODE
        return Collections.unmodifiableMap(map)
    }

    private fun hasCamera(): Boolean {
        return activity.packageManager
            .hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)
    }

    private fun setupCamera(cameraDirection: String, windowed: Boolean) {
        activity
            .runOnUiThread {
                val previewView = PreviewView(activity)
                previewView.layoutParams = FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.MATCH_PARENT
                )
                this.previewView = previewView

                val graphicOverlay = GraphicOverlay(activity)
                graphicOverlay.layoutParams = FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.MATCH_PARENT
                )
                this.graphicOverlay = graphicOverlay

                val parent = webView.parent as ViewGroup
                parent.addView(previewView)
                parent.addView(graphicOverlay)

                this.windowed = windowed
                if (windowed) {
                    webView.bringToFront()
                    webViewBackground = webView.background
                    webView.setBackgroundColor(Color.TRANSPARENT)
                }

                val cameraProviderFuture = ProcessCameraProvider.getInstance(activity)
                cameraProviderFuture.addListener(
                    {
                        try {
                            val cameraProvider = cameraProviderFuture.get()
                            bindPreview(
                                cameraProvider,
                                if (cameraDirection == "front") CameraSelector.LENS_FACING_FRONT else CameraSelector.LENS_FACING_BACK
                            )
                            this.cameraProvider = cameraProvider
                        } catch (e: InterruptedException) {
                            // ignored
                        } catch (_: ExecutionException) {
                            // ignored
                        }
                    },
                    ContextCompat.getMainExecutor(activity)
                )
                this.cameraProviderFuture = cameraProviderFuture
            }
    }

    private fun bindPreview(cameraProvider: ProcessCameraProvider, cameraDirection: Int) {
        activity
            .runOnUiThread {
                val preview = Preview.Builder().build()
                val cameraSelector =
                    CameraSelector.Builder().requireLensFacing(cameraDirection).build()
                preview.setSurfaceProvider(previewView?.surfaceProvider)
                val imageAnalysis = ImageAnalysis.Builder()
                    .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                    .setTargetResolution(Size(1280, 720))
                    .build()
                imageAnalysis.setAnalyzer(
                    ContextCompat.getMainExecutor(activity),
                    this
                )

                try {
                    camera = cameraProvider.bindToLifecycle(
                        activity as LifecycleOwner,
                        cameraSelector,
                        preview,
                        imageAnalysis
                    )
                } catch (e: Exception) {
                    // TODO
                }
            }
    }

    private fun dismantleCamera() {
        activity
            .runOnUiThread {
                if (cameraProvider != null) {
                    cameraProvider?.unbindAll()
                    val parent = webView.parent as ViewGroup
                    parent.removeView(previewView)
                    parent.removeView(graphicOverlay)
                    camera = null
                    previewView = null
                    graphicOverlay = null
                }
            }
    }

    private fun getFormats(args: ScanOptions): List<Int> {
        val formats = ArrayList<Int>()
        for (format in args.formats ?: arrayOf()) {
            val targetedBarcodeFormat = supportedFormats[format]
            if (targetedBarcodeFormat != null) {
                formats.add(targetedBarcodeFormat)
            }
        }
        return formats
    }

    private fun prepare(direction: String, windowed: Boolean) {
        dismantleCamera()
        setupCamera(direction, windowed)
    }

    private fun installBackHandler() {
        if (backCallback != null) return
        val host = activity as? ComponentActivity ?: return
        val callback = object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                // Same order as cancel(): settle the waiting scan before
                // destroy() clears it, then tear the preview down.
                savedInvoke?.reject("cancelled")
                destroy()
            }
        }
        host.onBackPressedDispatcher.addCallback(host, callback)
        backCallback = callback
    }

    private fun destroy() {
        backCallback?.remove()
        backCallback = null
        dismantleCamera()
        savedInvoke = null
        if (windowed) {
            if (webViewBackground != null) {
                webView.background = webViewBackground
                webViewBackground = null
            } else {
                webView.setBackgroundColor(Color.WHITE)
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun configureCamera(formats: List<Int>) {
        activity
            .runOnUiThread {
                val vibrator =
                    activity.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
                this.vibrator = vibrator
                // This used to throw when the preview was missing, but creating
                // the scanner has nothing to do with the preview - and now that
                // configureCamera() runs before prepare(), the preview is always
                // still null here.

                // FORMAT_ALL_FORMATS (-1) is not a valid argument to
                // setBarcodeFormats: MLKit throws, scanner stays null, and the
                // first frame is never released, which stalls the whole analysis
                // pipeline. "All formats" is expressed by not calling it at all.
                val builder = BarcodeScannerOptions.Builder()
                if (formats.isNotEmpty()) {
                    val wanted =
                        (listOf(Barcode.FORMAT_QR_CODE) + mapFormats(formats).toList()).distinct().toIntArray()
                    builder.setBarcodeFormats(wanted[0], *wanted.drop(1).toIntArray())
                }
                val options = builder.build()
                scannerOptions = options
                scanner = BarcodeScanning.getClient(options)
                Log.i("FluxyScan", "scanner ready, formats=" + formats)
            }
    }

    private fun mapFormats(integers: List<Int>): IntArray {
        // QR is always passed separately to setBarcodeFormats. The old version
        // allocated a full-size array and skipped QR, which left a literal 0 in
        // it - not a valid Barcode.FORMAT_* value. MLKit then filtered out every
        // code and reported no error, so scanning looked like it did nothing.
        return integers.filter { it != Barcode.FORMAT_QR_CODE }.toIntArray()
    }

    override fun analyze(image: ImageProxy) {
        Log.i("FluxyScan", "analyze frame " + image.width + "x" + image.height + " scanner=" + (scanner != null))
        @SuppressLint("UnsafeOptInUsageError") val mediaImage = image.image
        if (mediaImage == null) {
            Log.w("FluxyScan", "analyze: mediaImage is null, closing frame")
            image.close()
        }
        if (mediaImage != null) {
            val inputImage =
                InputImage.fromMediaImage(mediaImage, image.imageInfo.rotationDegrees)
            val activeScanner = scanner
            if (activeScanner == null) {
                Log.e("FluxyScan", "analyze called with scanner == null; releasing the frame")
                image.close()
                mediaImage.close()
                return
            }
            activeScanner
                .process(inputImage)
                ?.addOnSuccessListener { barcodes ->
                    Log.i("FluxyScan", "mlkit returned " + barcodes.size + " barcode(s)")
                    if (barcodes.isNotEmpty())  {
                        val barcode = barcodes[0]
                        val bounds = barcode.boundingBox
                        val rawValue = barcode.rawValue ?: ""
                        val rawFormat = barcode.format
                        var format: String? = null

                        for (entry in supportedFormats.entries) {
                            if (entry.value == rawFormat) {
                                format = entry.key
                                break
                            }
                        }

                        val s = bounds?.flattenToString()
                        val jsObject = JSObject()
                        jsObject.put("content", rawValue)
                        jsObject.put("format", format)
                        jsObject.put("bounds", s)

                        savedInvoke?.resolve(jsObject)
                        destroy()
                    }
                }
                ?.addOnFailureListener { e ->
                    Log.e("FluxyScan", "mlkit failed: " + e.message, e)
                    Logger.error(e.message ?: e.toString())
                }
                ?.addOnCompleteListener {
                    image.close()
                    mediaImage.close()
                }
        }
    }

    @Command
    fun vibrate(invoke: Invoke) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            vibrator!!.vibrate(
                VibrationEffect.createOneShot(
                    50,
                    VibrationEffect.DEFAULT_AMPLITUDE
                )
            )
        }
        invoke.resolve()
    }

    @Command
    fun cancel(invoke: Invoke) {
        // Reject the pending scan before destroy() clears savedInvoke, otherwise the
        // scan() promise is never settled. Mirrors the iOS implementation.
        savedInvoke?.reject("cancelled")
        destroy()
        invoke.resolve()
    }

    @Command
    fun scan(invoke: Invoke) {
        val args = invoke.parseArgs(ScanOptions::class.java)

        savedInvoke = invoke
        if (hasCamera()) {
            if (getPermissionState("camera") != PermissionState.GRANTED) {
                throw Exception("No permission to use camera. Did you request it yet?")
            } else {
                webViewBackground = null
                // configureCamera() runs after prepare()'s runOnUiThread block, but
                // frames can arrive before scanner is assigned. Configuring first
                // removes the race entirely.
                configureCamera(getFormats(args))
                prepare(args.cameraDirection ?: "back", args.windowed)
                // The dispatcher is UI-thread state; the command itself may not
                // be, and registering twice from two callers would double-reject.
                activity.runOnUiThread { installBackHandler() }
            }
        }
    }

    @SuppressLint("ObsoleteSdkInt")
    @PermissionCallback
    fun cameraPermissionCallback(invoke: Invoke) {
        if (requestPermissionResponse == null) {
            return
        }

        val requestPermissionResponse = requestPermissionResponse!!

        val granted = getPermissionState(PERMISSION_ALIAS_CAMERA) === PermissionState.GRANTED

        if (granted) {
            requestPermissionResponse.put(PERMISSION_ALIAS_CAMERA, PermissionState.GRANTED)
        } else {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                requestPermissionResponse.put(PERMISSION_ALIAS_CAMERA, PermissionState.DENIED)
            } else {
                requestPermissionResponse.put(PERMISSION_ALIAS_CAMERA, PermissionState.GRANTED)
            }
        }

        invoke.resolve(requestPermissionResponse)
        this.requestPermissionResponse = null
    }

    @SuppressLint("ObsoleteSdkInt")
    @Command
    override fun requestPermissions(invoke: Invoke) {
        val requestPermissionResponse = JSObject()
        this.requestPermissionResponse = requestPermissionResponse
        if (getPermissionState(PERMISSION_ALIAS_CAMERA) === PermissionState.GRANTED) {
            requestPermissionResponse.put(PERMISSION_ALIAS_CAMERA, PermissionState.GRANTED)
        } else {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                requestPermissionForAlias(
                    PERMISSION_ALIAS_CAMERA,
                    invoke,
                    "cameraPermissionCallback"
                )
                return
            } else {
                requestPermissionResponse.put(PERMISSION_ALIAS_CAMERA, PermissionState.GRANTED)
            }
        }
        invoke.resolve(requestPermissionResponse)
    }

    @Command
    fun openAppSettings(invoke: Invoke) {
        val intent = Intent(
            Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
            Uri.fromParts("package", activity.packageName, null)
        )
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivityForResult(invoke, intent, "openSettingsResult")
    }

    @ActivityCallback
    private fun openSettingsResult(invoke: Invoke, result: ActivityResult) {
        invoke.resolve()
    }

    companion object {
        @Volatile
        private var activeCamera: Camera? = null

        /**
         * The camera bound by an active scan, or `null` when no scan is running.
         *
         * Looked up through reflection by the torch plugin (`app.tauri.torch`): while a scan holds the
         * camera, Android rejects `CameraManager.setTorchMode`, so the flash must be driven through
         * this camera's `CameraControl` instead. Keep the name and signature stable.
         */
        @JvmStatic
        fun getActiveCamera(): Camera? = activeCamera
    }
}
