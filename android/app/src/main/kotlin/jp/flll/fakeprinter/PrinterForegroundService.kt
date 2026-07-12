package jp.flll.fakeprinter

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.util.Log
import java.io.File
import java.net.Inet4Address
import java.net.InetAddress
import java.net.NetworkInterface
import javax.jmdns.JmDNS
import javax.jmdns.ServiceInfo
import kotlin.concurrent.thread

/** Foreground service hosting the IPP server + mDNS/AirPrint advertisement. */
class PrinterForegroundService : Service() {

    private var core: IppServerCore? = null
    private var jmdns: JmDNS? = null
    private var multicastLock: WifiManager.MulticastLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (core != null) return START_STICKY // already running

        val prefs = Prefs(this)
        val address = localIpv4()
        startForeground(NOTIFICATION_ID, buildNotification(address, prefs.port))

        thread(name = "fake-printer-start") {
            try {
                val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
                multicastLock = wifi.createMulticastLock("fake-printer-mdns").apply {
                    setReferenceCounted(false)
                    acquire()
                }

                val server = IppServerCore(
                    port = prefs.port,
                    printerName = prefs.printerName,
                    saveDir = inboxDir(this),
                    log = { Log.i(TAG, it) },
                )
                server.start()
                core = server

                if (address != null) {
                    registerMdns(address, prefs.printerName, prefs.port)
                }
                Log.i(TAG, "printer up at ipp://${address?.hostAddress}:${prefs.port}/ipp/print")
            } catch (e: Exception) {
                Log.e(TAG, "failed to start printer", e)
                stopSelf()
            }
        }
        return START_STICKY
    }

    private fun registerMdns(address: InetAddress, name: String, port: Int) {
        val txt = mapOf(
            "txtvers" to "1",
            "qtotal" to "1",
            "rp" to "ipp/print",
            "ty" to name,
            "product" to "(fake-printer android)",
            "note" to "Saves print jobs as PDF",
            "pdl" to "application/pdf,image/pwg-raster,image/urf",
            "URF" to "W8,SRGB24,CP255,DM1,FN3,IS0-0,MT1-8-11,OB10,PQ4-5,RS300,ST13,V1.4,W8",
            "Color" to "T",
            "Duplex" to "F",
        )
        val dns = JmDNS.create(address, name.replace(" ", "-"))
        // Registering with the "universal" subtype announces both the plain
        // _ipp._tcp PTR (Android/Windows) and _universal._sub._ipp._tcp (AirPrint).
        dns.registerService(ServiceInfo.create("_ipp._tcp.local.", name, "universal", port, 0, 0, txt))
        jmdns = dns
        Log.i(TAG, "mDNS registered: $name on ${address.hostAddress}:$port")
    }

    override fun onDestroy() {
        thread(name = "fake-printer-stop") {
            try { jmdns?.unregisterAllServices(); jmdns?.close() } catch (_: Exception) {}
            try { core?.stop() } catch (_: Exception) {}
            try { multicastLock?.release() } catch (_: Exception) {}
        }
        core = null
        jmdns = null
        super.onDestroy()
    }

    private fun buildNotification(address: InetAddress?, port: Int): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= 26) {
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Fake Printer", NotificationManager.IMPORTANCE_LOW)
            )
        }
        val contentIntent = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val builder = if (Build.VERSION.SDK_INT >= 26) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION") Notification.Builder(this)
        }
        return builder
            .setSmallIcon(android.R.drawable.ic_menu_share)
            .setContentTitle("Fake Printer 稼働中")
            .setContentText("ipp://${address?.hostAddress ?: "?"}:$port/ipp/print")
            .setContentIntent(contentIntent)
            .setOngoing(true)
            .build()
    }

    companion object {
        private const val TAG = "FakePrinterService"
        private const val CHANNEL_ID = "fake_printer"
        private const val NOTIFICATION_ID = 1

        fun inboxDir(context: Context): File =
            File(context.getExternalFilesDir(null), "inbox")

        fun localIpv4(): InetAddress? =
            NetworkInterface.getNetworkInterfaces().asSequence()
                .filter { it.isUp && !it.isLoopback }
                .flatMap { it.inetAddresses.asSequence() }
                .filterIsInstance<Inet4Address>()
                .firstOrNull { it.isSiteLocalAddress }
    }
}
