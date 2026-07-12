package jp.flll.fakeprinter

import android.content.Context
import android.os.Build

class Prefs(context: Context) {
    private val sp = context.getSharedPreferences("fake_printer", Context.MODE_PRIVATE)

    var printerName: String
        get() = sp.getString("printer_name", defaultName) ?: defaultName
        set(value) = sp.edit().putString("printer_name", value.trim()).apply()

    var port: Int
        get() = sp.getInt("port", DEFAULT_PORT)
        set(value) = sp.edit().putInt("port", value).apply()

    var forwardEnabled: Boolean
        get() = sp.getBoolean("forward_enabled", true)
        set(value) = sp.edit().putBoolean("forward_enabled", value).apply()

    var forwardHost: String
        get() = sp.getString("forward_host", DEFAULT_FORWARD_HOST) ?: DEFAULT_FORWARD_HOST
        set(value) = sp.edit().putString("forward_host", value.trim()).apply()

    var forwardPort: Int
        get() = sp.getInt("forward_port", DEFAULT_PORT)
        set(value) = sp.edit().putInt("forward_port", value).apply()

    private val defaultName = "Fake Printer (${Build.MODEL ?: "Android"})"

    companion object {
        const val DEFAULT_PORT = 8631
        const val DEFAULT_FORWARD_HOST = "192.168.30.11"
    }
}

/** 未転送キューの SharedPreferences 実装 */
class PrefsPendingStore(context: Context) : PendingStore {
    private val sp = context.getSharedPreferences("fake_printer_forward", Context.MODE_PRIVATE)

    override fun load(): Set<String> =
        sp.getStringSet("pending", emptySet())?.toSet() ?: emptySet()

    override fun save(names: Set<String>) {
        sp.edit().putStringSet("pending", names.toSet()).apply()
    }
}
