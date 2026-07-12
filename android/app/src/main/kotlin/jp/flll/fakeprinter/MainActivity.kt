package jp.flll.fakeprinter

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.pdf.PdfRenderer
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.text.TextUtils
import android.text.format.Formatter
import android.util.LruCache
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.BaseAdapter
import android.widget.Button
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.TextView
import android.widget.Toast
import androidx.core.content.FileProvider
import java.io.File
import java.io.FileOutputStream
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.Executors

/** 受信ダッシュボード（LDSG 準拠）: サーバ開始/停止 + 受信 PDF のサムネイル一覧 */
class MainActivity : Activity() {

    private lateinit var statusTitle: TextView
    private lateinit var statusDetail: TextView
    private lateinit var toggleButton: Button
    private lateinit var listView: ListView
    private lateinit var emptyText: TextView
    private lateinit var adapter: InboxAdapter
    private var files: List<File> = emptyList()
    private var serverRunning = false
    private val handler = Handler(Looper.getMainLooper())
    private val refreshLoop = object : Runnable {
        override fun run() {
            refreshFiles()
            handler.postDelayed(this, 3000)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val s2 = Ldsg.space(this, R.dimen.ldsg_space_2)
        val s3 = Ldsg.space(this, R.dimen.ldsg_space_3)
        val s4 = Ldsg.space(this, R.dimen.ldsg_space_4)
        val s5 = Ldsg.space(this, R.dimen.ldsg_space_5)

        // ---- ヘッダー
        val header = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(Ldsg.title(this@MainActivity, "Fake Printer", R.dimen.ldsg_fs_title_xl),
                LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            addView(Ldsg.secondaryButton(this@MainActivity, "設定").apply {
                setOnClickListener {
                    startActivity(Intent(this@MainActivity, SettingsActivity::class.java))
                }
            })
        }

        // ---- ステータスカード
        statusTitle = Ldsg.title(this, "", R.dimen.ldsg_fs_title_m)
        statusDetail = Ldsg.text(this, "", R.dimen.ldsg_fs_text_s, R.color.ldsg_text_sub)
        toggleButton = Ldsg.primaryButton(this, "プリンタを開始").apply {
            setOnClickListener { toggleServer() }
        }
        val statusCard = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = Ldsg.cardBackground(this@MainActivity)
            setPadding(s4, s4, s4, s4)
            addView(statusTitle)
            addView(statusDetail, LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = s2 })
            addView(toggleButton, LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = s3 })
        }

        // ---- 受信一覧
        val sectionRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.BOTTOM
            addView(Ldsg.title(this@MainActivity, "受信した文書", R.dimen.ldsg_fs_title_m),
                LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            addView(Ldsg.text(this@MainActivity, "タップで表示 / 長押しで削除",
                R.dimen.ldsg_fs_text_xs, R.color.ldsg_text_tertiary))
        }
        emptyText = Ldsg.text(this, "まだ何も受信していません",
            R.dimen.ldsg_fs_text_m, R.color.ldsg_text_sub).apply {
            gravity = Gravity.CENTER
            setPadding(0, s5, 0, s5)
        }
        adapter = InboxAdapter()
        listView = ListView(this).apply {
            divider = null
            adapter = this@MainActivity.adapter
            emptyView = emptyText
            setOnItemClickListener { _, _, position, _ -> openFile(files[position]) }
            setOnItemLongClickListener { _, _, position, _ -> confirmDelete(files[position]); true }
        }
        val listContainer = FrameLayout(this).apply {
            addView(listView)
            addView(emptyText)
        }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(s4, s4, s4, s4)
            addView(header)
            addView(statusCard, LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = s3 })
            addView(sectionRow, LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = s5; bottomMargin = s2 })
            addView(listContainer, LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        }
        setContentView(root)

        if (android.os.Build.VERSION.SDK_INT >= 33) {
            requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        serverRunning = false
        updateStatus()
        toggleServer() // アプリを開いたらプリンタを自動起動
    }

    override fun onResume() {
        super.onResume()
        handler.post(refreshLoop)
        updateStatus()
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
        if (serverRunning) {
            statusTitle.text = "🟢 稼働中"
            statusDetail.text = "ipp://$ip:${prefs.port}/ipp/print\n" +
                "他の端末の印刷画面に「${prefs.printerName}」が表示されます" +
                if (prefs.forwardEnabled) "\nPC (${prefs.forwardHost}) へ自動転送: ON" else ""
            toggleButton.text = "プリンタを停止"
        } else {
            statusTitle.text = "⚪ 停止中"
            statusDetail.text = "開始すると LAN にプリンタとして公開されます"
            toggleButton.text = "プリンタを開始"
        }
    }

    private fun refreshFiles() {
        val dir = PrinterForegroundService.inboxDir(this)
        val latest = dir.listFiles()?.sortedByDescending { it.lastModified() } ?: emptyList()
        if (latest.map { it.name to it.length() } != files.map { it.name to it.length() }) {
            files = latest
            adapter.notifyDataSetChanged()
        }
    }

    private fun openFile(file: File) {
        if (file.extension == "pdf") {
            startActivity(Intent(this, PdfViewerActivity::class.java)
                .putExtra(PdfViewerActivity.EXTRA_PATH, file.absolutePath))
        } else {
            shareFile(file)
        }
    }

    private fun shareFile(file: File) {
        val uri: Uri = FileProvider.getUriForFile(this, "$packageName.files", file)
        val intent = Intent(Intent.ACTION_SEND)
            .setType("application/octet-stream")
            .putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        try {
            startActivity(Intent.createChooser(intent, file.name))
        } catch (_: Exception) {
            Toast.makeText(this, "共有先がありません", Toast.LENGTH_SHORT).show()
        }
    }

    private fun confirmDelete(file: File) {
        AlertDialog.Builder(this)
            .setMessage("${file.name} を削除しますか？")
            .setPositiveButton("削除") { _, _ ->
                file.delete()
                ThumbLoader.evict(this, file.name)
                refreshFiles()
            }
            .setNegativeButton("キャンセル", null)
            .show()
    }

    // ------------------------------------------------------------- adapter

    private inner class InboxAdapter : BaseAdapter() {
        private val dateFormat = SimpleDateFormat("MM/dd HH:mm", Locale.JAPAN)
        private val pendingStore = PrefsPendingStore(this@MainActivity)

        override fun getCount() = files.size
        override fun getItem(position: Int) = files[position]
        override fun getItemId(position: Int) = files[position].name.hashCode().toLong()

        override fun getView(position: Int, convertView: View?, parent: ViewGroup): View {
            val holder: RowHolder
            val row: FrameLayout
            if (convertView == null) {
                holder = RowHolder(this@MainActivity)
                row = holder.root
                row.tag = holder
            } else {
                row = convertView as FrameLayout
                holder = row.tag as RowHolder
            }
            val file = files[position]
            holder.name.text = file.name
            holder.meta.text = "${dateFormat.format(Date(file.lastModified()))}  " +
                Formatter.formatShortFileSize(this@MainActivity, file.length())

            val prefs = Prefs(this@MainActivity)
            when {
                file.extension != "pdf" -> { holder.badge.text = "" }
                !prefs.forwardEnabled -> { holder.badge.text = "" }
                pendingStore.load().contains(file.name) -> {
                    holder.badge.text = "転送待ち"
                    holder.badge.setTextColor(Ldsg.color(this@MainActivity, R.color.ldsg_text_tertiary))
                }
                else -> {
                    holder.badge.text = "✓ PC転送済み"
                    holder.badge.setTextColor(Ldsg.color(this@MainActivity, R.color.ldsg_brand_primary_alt))
                }
            }
            ThumbLoader.load(this@MainActivity, file, holder.thumb)
            return row
        }
    }

    /** 一覧の行: カード（サムネ + 名前 + メタ + バッジ） */
    private class RowHolder(activity: MainActivity) {
        val root: FrameLayout
        val thumb: ImageView
        val name: TextView
        val meta: TextView
        val badge: TextView

        init {
            val c = activity
            val s2 = Ldsg.space(c, R.dimen.ldsg_space_2)
            val s3 = Ldsg.space(c, R.dimen.ldsg_space_3)
            val thumbW = Ldsg.space(c, R.dimen.ldsg_space_7) // 48dp
            val thumbH = Ldsg.space(c, R.dimen.ldsg_space_8) // 64dp

            thumb = ImageView(c).apply {
                scaleType = ImageView.ScaleType.CENTER_CROP
                background = Ldsg.cardBackground(c, R.color.ldsg_surface_overlay)
                clipToOutline = true
            }
            name = Ldsg.text(c, "", R.dimen.ldsg_fs_text_m).apply {
                typeface = android.graphics.Typeface.DEFAULT_BOLD
                maxLines = 1
                ellipsize = TextUtils.TruncateAt.MIDDLE
            }
            meta = Ldsg.text(c, "", R.dimen.ldsg_fs_text_s, R.color.ldsg_text_sub)
            badge = Ldsg.text(c, "", R.dimen.ldsg_fs_text_s).apply {
                typeface = android.graphics.Typeface.DEFAULT_BOLD
            }

            val column = LinearLayout(c).apply {
                orientation = LinearLayout.VERTICAL
                addView(name)
                addView(meta)
                addView(badge)
            }
            val card = LinearLayout(c).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                background = Ldsg.cardBackground(c)
                setPadding(s3, s3, s3, s3)
                addView(thumb, LinearLayout.LayoutParams(thumbW, thumbH))
                addView(column, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
                    .apply { marginStart = s3 })
            }
            root = FrameLayout(c).apply {
                setPadding(0, 0, 0, s2)
                addView(card)
            }
        }
    }

    /** サムネイル: PdfRenderer で1ページ目を描画し、メモリ + ディスクにキャッシュ */
    private object ThumbLoader {
        private val executor = Executors.newFixedThreadPool(2)
        private val main = Handler(Looper.getMainLooper())
        private val memory = LruCache<String, Bitmap>(32)

        fun load(activity: Activity, file: File, target: ImageView) {
            target.tag = file.name
            memory.get(file.name)?.let { target.setImageBitmap(it); return }
            target.setImageBitmap(null)
            if (file.extension != "pdf") return
            executor.execute {
                val bitmap = fromDisk(activity, file) ?: render(activity, file)
                if (bitmap != null) {
                    memory.put(file.name, bitmap)
                    main.post { if (target.tag == file.name) target.setImageBitmap(bitmap) }
                }
            }
        }

        fun evict(activity: Activity, name: String) {
            memory.remove(name)
            File(thumbDir(activity), "$name.png").delete()
        }

        private fun thumbDir(activity: Activity): File =
            File(activity.cacheDir, "thumbs").apply { mkdirs() }

        private fun fromDisk(activity: Activity, file: File): Bitmap? {
            val cached = File(thumbDir(activity), "${file.name}.png")
            if (!cached.exists() || cached.lastModified() < file.lastModified()) return null
            return android.graphics.BitmapFactory.decodeFile(cached.absolutePath)
        }

        private fun render(activity: Activity, file: File): Bitmap? = try {
            ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { pfd ->
                PdfRenderer(pfd).use { renderer ->
                    renderer.openPage(0).use { page ->
                        val w = 192
                        val h = (w.toFloat() * page.height / page.width).toInt().coerceAtLeast(1)
                        Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888).also { bmp ->
                            bmp.eraseColor(android.graphics.Color.WHITE)
                            page.render(bmp, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                            FileOutputStream(File(thumbDir(activity), "${file.name}.png")).use { out ->
                                bmp.compress(Bitmap.CompressFormat.PNG, 90, out)
                            }
                        }
                    }
                }
            }
        } catch (_: Exception) {
            null
        }
    }
}
