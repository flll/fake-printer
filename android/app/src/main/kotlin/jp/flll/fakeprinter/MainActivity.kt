package jp.flll.fakeprinter

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.text.format.Formatter
import android.view.Gravity
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.TextView
import android.widget.Toast
import androidx.core.content.FileProvider
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** Receiver dashboard: server on/off + list of received PDFs (tap=open, long-press=delete). */
class MainActivity : Activity() {

    private lateinit var statusText: TextView
    private lateinit var toggleButton: Button
    private lateinit var listView: ListView
    private var files: List<File> = emptyList()
    private val handler = Handler(Looper.getMainLooper())
    private var serverRunning = false
    private val refreshLoop = object : Runnable {
        override fun run() {
            refreshFiles()
            handler.postDelayed(this, 3000)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val pad = (12 * resources.displayMetrics.density).toInt()

        statusText = TextView(this)
        toggleButton = Button(this)
        val settingsButton = Button(this).apply {
            text = "設定"
            setOnClickListener { startActivity(Intent(this@MainActivity, SettingsActivity::class.java)) }
        }
        toggleButton.setOnClickListener { toggleServer() }

        val buttonRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(toggleButton)
            addView(settingsButton)
        }

        listView = ListView(this).apply {
            setOnItemClickListener { _, _, position, _ -> openFile(files[position]) }
            setOnItemLongClickListener { _, _, position, _ -> confirmDelete(files[position]); true }
        }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
            addView(statusText)
            addView(buttonRow)
            addView(TextView(this@MainActivity).apply { text = "受信した文書（タップで開く / 長押しで削除）" })
            addView(listView, LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f))
        }
        setContentView(root)

        if (android.os.Build.VERSION.SDK_INT >= 33) {
            requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        serverRunning = false
        updateStatus()
        toggleServer() // auto-start the printer when the app opens
    }

    override fun onResume() {
        super.onResume()
        handler.post(refreshLoop)
    }

    override fun onPause() {
        handler.removeCallbacks(refreshLoop)
        super.onPause()
    }

    private fun toggleServer() {
        val intent = Intent(this, PrinterForegroundService::class.java)
        if (serverRunning) {
            stopService(intent)
            serverRunning = false
        } else {
            if (android.os.Build.VERSION.SDK_INT >= 26) startForegroundService(intent)
            else startService(intent)
            serverRunning = true
        }
        updateStatus()
    }

    private fun updateStatus() {
        val prefs = Prefs(this)
        val ip = PrinterForegroundService.localIpv4()?.hostAddress ?: "?"
        statusText.text = if (serverRunning) {
            "🟢 稼働中: ipp://$ip:${prefs.port}/ipp/print\n他の端末の印刷画面に「${prefs.printerName}」が表示されます"
        } else {
            "⚪ 停止中"
        }
        toggleButton.text = if (serverRunning) "プリンタを停止" else "プリンタを開始"
    }

    private fun refreshFiles() {
        val dir = PrinterForegroundService.inboxDir(this)
        files = dir.listFiles()?.sortedByDescending { it.lastModified() } ?: emptyList()
        val fmt = SimpleDateFormat("MM/dd HH:mm", Locale.JAPAN)
        val labels = files.map {
            "${it.name}\n${fmt.format(Date(it.lastModified()))}  ${Formatter.formatShortFileSize(this, it.length())}"
        }
        listView.adapter = ArrayAdapter(this, android.R.layout.simple_list_item_1, labels)
    }

    private fun openFile(file: File) {
        val uri: Uri = FileProvider.getUriForFile(this, "$packageName.files", file)
        val mime = if (file.extension == "pdf") "application/pdf" else "application/octet-stream"
        val intent = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, mime)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        try {
            startActivity(intent)
        } catch (_: Exception) {
            Toast.makeText(this, "PDF を開けるアプリがありません", Toast.LENGTH_SHORT).show()
        }
    }

    private fun confirmDelete(file: File) {
        AlertDialog.Builder(this)
            .setMessage("${file.name} を削除しますか？")
            .setPositiveButton("削除") { _, _ -> file.delete(); refreshFiles() }
            .setNegativeButton("キャンセル", null)
            .show()
    }
}
