package com.thep0y.fluxy.mobile

import android.app.Activity
import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import android.util.Log
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Service discovery through NsdManager (docs/MOBILE_PLAN.md 5.1).
 *
 * The TXT keys are the desktop's, not invented here: discovery.rs publishes
 * id, name, os, v, features and fp, so a peer written with any other spelling is
 * invisible to it. Reading the same keys is what makes both directions work.
 */
class Discovery(private val activity: Activity) {
    private val manager by lazy {
        activity.getSystemService(Context.NSD_SERVICE) as NsdManager
    }
    private val localNetwork by lazy { LocalNetwork(activity) }
    // Without this the Wi-Fi chip drops multicast and broadcast frames addressed
    // to us, so the UDP fallback hears nothing even though the socket is bound.
    private var multicast: WifiManager.MulticastLock? = null
    private val peers = CopyOnWriteArrayList<Map<String, Any?>>()
    private val resolving = CopyOnWriteArrayList<String>()
    private var registration: NsdManager.RegistrationListener? = null
    private var browser: NsdManager.DiscoveryListener? = null

    fun start(port: Int, deviceId: String, name: String, fingerprint: String): Boolean {
        stop()
        if (!localNetwork.granted()) {
            Log.w(TAG, "no local network permission yet")
            localNetwork.request()
            return false
        }
        val info = NsdServiceInfo().apply {
            serviceName = "fluxy-" + deviceId.take(8)
            serviceType = SERVICE_TYPE
            setPort(port)
            setAttribute(KEY_ID, deviceId)
            setAttribute(KEY_NAME, name)
            setAttribute(KEY_OS, "android")
            setAttribute(KEY_VERSION, PROTOCOL_VERSION)
            setAttribute(KEY_FEATURES, "chat")
            setAttribute(KEY_FINGERPRINT, fingerprint)
        }
        acquireMulticast()
        val listener = object : NsdManager.RegistrationListener {
            override fun onServiceRegistered(service: NsdServiceInfo) {
                Log.i(TAG, "registered " + service.serviceName)
            }
            override fun onRegistrationFailed(service: NsdServiceInfo, errorCode: Int) {
                Log.w(TAG, "registration failed: " + errorCode)
            }
            override fun onServiceUnregistered(service: NsdServiceInfo) {
                Log.i(TAG, "unregistered")
            }
            override fun onUnregistrationFailed(service: NsdServiceInfo, errorCode: Int) {
                Log.w(TAG, "unregistration failed: " + errorCode)
            }
        }
        return runCatching {
            manager.registerService(info, NsdManager.PROTOCOL_DNS_SD, listener)
            registration = listener
            browse()
            true
        }.onFailure {
            Log.w(TAG, "register threw: " + it.message)
        }.getOrDefault(false)
    }

    private fun browse() {
        val listener = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) {
                Log.i(TAG, "browsing " + serviceType)
            }
            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
                Log.w(TAG, "discovery failed to start: " + errorCode)
            }
            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {
                Log.w(TAG, "discovery failed to stop: " + errorCode)
            }
            override fun onDiscoveryStopped(serviceType: String) {
                Log.i(TAG, "discovery stopped")
            }
            override fun onServiceLost(service: NsdServiceInfo) {
                peers.removeAll { it["service"] == service.serviceName }
            }
            override fun onServiceFound(service: NsdServiceInfo) {
                Log.i(TAG, "found " + service.serviceName + " " + service.serviceType)
                // NsdManager resolves one service at a time; a second call while
                // one is in flight fails with FAILURE_ALREADY_ACTIVE.
                resolve(service)
            }
        }
        runCatching {
            manager.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, listener)
            browser = listener
        }.onFailure { Log.w(TAG, "browse threw: " + it.message) }
    }

    private fun resolve(service: NsdServiceInfo) {
        if (!resolving.addIfAbsent(service.serviceName)) return
        runCatching {
            manager.resolveService(service, object : NsdManager.ResolveListener {
                override fun onResolveFailed(info: NsdServiceInfo, errorCode: Int) {
                    resolving.remove(info.serviceName)
                    Log.w(TAG, "resolve failed for " + info.serviceName + ": " + errorCode)
                }

                override fun onServiceResolved(info: NsdServiceInfo) {
                    resolving.remove(info.serviceName)
                    val attributes = info.attributes ?: emptyMap()
                    fun text(key: String): String? =
                        attributes[key]?.toString(Charsets.UTF_8)
                    val deviceId = text(KEY_ID)
                    if (deviceId == null) {
                        Log.w(TAG, "resolved " + info.serviceName + " without an id")
                        return
                    }
                    // Link-local IPv6 arrives as `fe80::1%wlan0`, which is not an IP
                    // literal the other side can parse.
                    val address = info.host?.hostAddress?.substringBefore('%')
                    if (address == null) {
                        Log.w(TAG, "resolved " + info.serviceName + " without an address")
                        return
                    }
                    val entry = linkedMapOf<String, Any?>(
                        "service" to info.serviceName,
                        "device_id" to deviceId,
                        "name" to (text(KEY_NAME) ?: info.serviceName),
                        "platform" to (text(KEY_OS) ?: "android"),
                        "fingerprint" to (text(KEY_FINGERPRINT) ?: ""),
                        "address" to address,
                        "port" to info.port,
                    )
                    Log.i(
                        TAG,
                        "resolved " + info.serviceName + " at " + address + ":" + info.port,
                    )
                    peers.removeAll { it["device_id"] == deviceId }
                    peers.add(entry)
                }
            })
        }.onFailure {
            resolving.remove(service.serviceName)
            Log.w(TAG, "resolve threw: " + it.message)
        }
    }

    private fun acquireMulticast() {
        if (multicast != null) return
        runCatching {
            val wifi = activity.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            val lock = wifi.createMulticastLock("fluxy-discovery")
            lock.setReferenceCounted(false)
            lock.acquire()
            multicast = lock
        }.onFailure { Log.w(TAG, "multicast lock refused: " + it.message) }
    }

    private fun releaseMulticast() {
        multicast?.let { runCatching { it.release() } }
        multicast = null
    }

    fun stop() {
        releaseMulticast()
        registration?.let { runCatching { manager.unregisterService(it) } }
        browser?.let { runCatching { manager.stopServiceDiscovery(it) } }
        registration = null
        browser = null
        peers.clear()
        resolving.clear()
    }

    fun snapshot(): List<Map<String, Any?>> = peers.toList()

    companion object {
        private const val TAG = "FluxyDiscovery"

        /** NsdManager appends .local. itself. */
        const val SERVICE_TYPE = "_fluxy._udp."

        // The desktop's keys, so both sides read each other.
        const val KEY_ID = "id"
        const val KEY_NAME = "name"
        const val KEY_OS = "os"
        const val KEY_VERSION = "v"
        const val KEY_FEATURES = "features"
        const val KEY_FINGERPRINT = "fp"
        const val PROTOCOL_VERSION = "1"
    }
}
