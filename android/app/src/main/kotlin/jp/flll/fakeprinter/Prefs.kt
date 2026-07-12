package jp.flll.fakeprinter

import android.content.Context

class Prefs(context: Context) {
    private val sp = context.getSharedPreferences("fake_printer", Context.MODE_PRIVATE)

    var host: String
        get() = sp.getString("host", DEFAULT_HOST) ?: DEFAULT_HOST
        set(value) = sp.edit().putString("host", value.trim()).apply()

    var port: Int
        get() = sp.getInt("port", DEFAULT_PORT)
        set(value) = sp.edit().putInt("port", value).apply()

    var path: String
        get() = sp.getString("path", DEFAULT_PATH) ?: DEFAULT_PATH
        set(value) = sp.edit().putString("path", value.trim()).apply()

    companion object {
        const val DEFAULT_HOST = "192.168.30.11"
        const val DEFAULT_PORT = 8631
        const val DEFAULT_PATH = "/ipp/print"
    }
}
