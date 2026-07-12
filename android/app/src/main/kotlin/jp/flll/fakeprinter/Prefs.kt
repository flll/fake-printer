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

    private val defaultName = "Fake Printer (${Build.MODEL ?: "Android"})"

    companion object {
        const val DEFAULT_PORT = 8631
    }
}
