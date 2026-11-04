package com.thep0y.fluxy.mobile

import android.app.Activity
import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities

/**
 * What the platform knows about the active network.
 *
 * The desktop build reads the interface table and the default route from the OS;
 * Android hides both from an app. This is the replacement: the gateway the fallback
 * probe should aim at, the interface carrying it, and whether the link is Wi-Fi.
 */
class NetworkInfo(private val activity: Activity) {
    fun describe(): Map<String, Any?> {
        val manager = activity.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
            ?: return empty()
        val network = manager.activeNetwork ?: return empty()
        val properties = manager.getLinkProperties(network) ?: return empty()
        val capabilities = manager.getNetworkCapabilities(network)
        // Prefer an IPv4 gateway: the fallback probe broadcasts over IPv4, and a
        // link-local IPv6 default route is common on emulators and some phones.
        val gateways = properties.routes
            .filter { it.isDefaultRoute }
            .mapNotNull { it.gateway?.hostAddress }
        val gateway = gateways.firstOrNull { it.contains('.') } ?: gateways.firstOrNull()
        val wifi = capabilities?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true
        // A phone acting as a hotspot is not the active-network role, so the app
        // sees the same Wi-Fi transport; the distinction is reported by the
        // settings screen instead of guessed here.
        return mapOf(
            "gateway" to gateway,
            "interface" to properties.interfaceName,
            "wifi" to wifi,
            "hotspot" to false,
            "addresses" to properties.linkAddresses.mapNotNull { it.address.hostAddress },
        )
    }

    private fun empty(): Map<String, Any?> = mapOf(
        "gateway" to null,
        "interface" to null,
        "wifi" to false,
        "hotspot" to false,
        "addresses" to emptyList<String>(),
    )
}
