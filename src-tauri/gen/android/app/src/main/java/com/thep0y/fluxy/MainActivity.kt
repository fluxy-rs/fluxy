package com.thep0y.fluxy

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import com.thep0y.fluxy.mobile.PendingShare

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // A share or a fluxy:// link can be the reason the activity started.
    capture(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    // singleTask: a second share arrives here rather than through onCreate.
    android.util.Log.i("FluxyShare", "onNewIntent action=" + intent.action + " data=" + intent.data)
    capture(intent)
  }

  override fun onResume() {
    super.onResume()
    // A share that arrived while the task was already on top can surface here
    // instead of onNewIntent, so capture whatever the activity currently holds.
    android.util.Log.i("FluxyShare", "onResume action=" + intent?.action + " data=" + intent?.data)
    capture(intent)
  }

  private fun capture(intent: Intent?) {
    // A deep link supersedes the ACTION_SEND path; otherwise take the share.
    if (!PendingShare.captureLink(intent?.data)) PendingShare.capture(intent)
    // A notification tap arrives as the notification plugin's PendingIntent,
    // which carries its notification id. That is the only marker available to
    // us, so use it to open the message list instead of wherever the app was.
    if (intent != null && intent.hasExtra("NotificationId")) {
      // The notification plugin puts the whole notification JSON on the
      // PendingIntent; our chat notifications carry the device id inside it, so
      // a tap can land on that conversation rather than just the list.
      val deviceId = deviceIdFromNotification(intent)
      android.util.Log.i("FluxyShare", "notification tap device=" + deviceId + " raw=" + intent.getStringExtra("LocalNotficationObject"))
      PendingShare.requestOpenChat(deviceId)
    }
  }

  private fun deviceIdFromNotification(intent: Intent): String? = try {
    val raw = intent.getStringExtra("LocalNotficationObject")
    if (raw == null) {
      null
    } else {
      org.json.JSONObject(raw).optJSONObject("extra")?.optString("device_id")?.takeIf { it.isNotEmpty() }
    }
  } catch (_: Exception) {
    null
  }
}
