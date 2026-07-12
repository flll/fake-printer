package jp.flll.fakeprinter

import android.app.Activity
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.pdf.PdfRenderer
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.text.TextUtils
import android.view.GestureDetector
import android.view.Gravity
import android.view.MotionEvent
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import androidx.core.content.FileProvider
import java.io.File
import java.util.concurrent.Executors
import kotlin.math.abs

/** アプリ内 PDF ビューア: 1ページ表示 + 左右スワイプ/タップでページ送り + 共有 */
class PdfViewerActivity : Activity() {

    private lateinit var file: File
    private lateinit var image: ImageView
    private lateinit var indicator: TextView
    private var pfd: ParcelFileDescriptor? = null
    private var renderer: PdfRenderer? = null
    private var pageIndex = 0
    private var pageCount = 0
    private val executor = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val path = intent.getStringExtra(EXTRA_PATH)
        if (path == null) { finish(); return }
        file = File(path)

        val s3 = Ldsg.space(this, R.dimen.ldsg_space_3)
        val s4 = Ldsg.space(this, R.dimen.ldsg_space_4)

        val backButton = Ldsg.secondaryButton(this, "←").apply { setOnClickListener { finish() } }
        val titleView = Ldsg.title(this, file.name, R.dimen.ldsg_fs_title_m).apply {
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.MIDDLE
        }
        val shareButton = Ldsg.primaryButton(this, "共有").apply { setOnClickListener { share() } }

        val topBar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(backButton)
            addView(titleView, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                .apply { marginStart = s3; marginEnd = s3 })
            addView(shareButton)
        }

        image = ImageView(this).apply { scaleType = ImageView.ScaleType.FIT_CENTER }
        indicator = Ldsg.text(this, "", R.dimen.ldsg_fs_text_s, R.color.ldsg_text_sub)
            .apply { gravity = Gravity.CENTER }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(s4, s4, s4, s4)
            addView(topBar)
            addView(image, LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f).apply { topMargin = s3 })
            addView(indicator, LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = s3 })
        }
        setContentView(root)

        val gesture = GestureDetector(this, object : GestureDetector.SimpleOnGestureListener() {
            override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
                if (abs(vx) > abs(vy)) {
                    if (vx < 0) showPage(pageIndex + 1) else showPage(pageIndex - 1)
                    return true
                }
                return false
            }

            override fun onSingleTapUp(e: MotionEvent): Boolean {
                val third = image.width / 3f
                when {
                    e.x < third -> showPage(pageIndex - 1)
                    e.x > third * 2 -> showPage(pageIndex + 1)
                }
                return true
            }
        })
        image.setOnTouchListener { _, event -> gesture.onTouchEvent(event); true }

        try {
            pfd = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
            renderer = PdfRenderer(pfd!!)
            pageCount = renderer!!.pageCount
        } catch (e: Exception) {
            Toast.makeText(this, "PDF を開けません: ${e.message}", Toast.LENGTH_SHORT).show()
            finish()
            return
        }
        image.post { showPage(0) }
    }

    private fun showPage(index: Int) {
        val target = index.coerceIn(0, pageCount - 1)
        pageIndex = target
        val viewWidth = image.width.coerceIn(320, 1600)
        executor.execute {
            val bitmap = try {
                renderer?.let { r ->
                    // PdfRenderer はページを同時に1つしか開けない(単一 executor で直列化)
                    r.openPage(target).use { page ->
                        val w = viewWidth
                        val h = (w.toFloat() * page.height / page.width).toInt().coerceAtLeast(1)
                        Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888).also { bmp ->
                            // 紙の再現なのでテーマに関わらず白（透過PDFの黒文字対策）
                            bmp.eraseColor(android.graphics.Color.WHITE)
                            page.render(bmp, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                        }
                    }
                }
            } catch (_: Exception) {
                null
            }
            main.post {
                if (bitmap != null) {
                    image.setImageBitmap(bitmap)
                    indicator.text = "${target + 1} / $pageCount"
                }
            }
        }
    }

    private fun share() {
        val uri = FileProvider.getUriForFile(this, "$packageName.files", file)
        val intent = Intent(Intent.ACTION_SEND)
            .setType("application/pdf")
            .putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        startActivity(Intent.createChooser(intent, file.name))
    }

    override fun onDestroy() {
        executor.shutdown()
        try { renderer?.close() } catch (_: Exception) {}
        try { pfd?.close() } catch (_: Exception) {}
        super.onDestroy()
    }

    companion object {
        const val EXTRA_PATH = "path"
    }
}
