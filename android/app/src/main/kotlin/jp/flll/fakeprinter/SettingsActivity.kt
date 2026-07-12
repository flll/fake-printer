package jp.flll.fakeprinter

import android.app.Activity
import android.os.Bundle
import android.text.InputType
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast

/** Minimal settings screen: fake-printer host / port / IPP path. */
class SettingsActivity : Activity() {

    private lateinit var hostInput: EditText
    private lateinit var portInput: EditText
    private lateinit var pathInput: EditText

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val prefs = Prefs(this)
        val pad = (16 * resources.displayMetrics.density).toInt()

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
        }

        fun label(text: String) = root.addView(TextView(this).apply { this.text = text })

        label("サーバ IP / ホスト名")
        hostInput = EditText(this).apply { setText(prefs.host) }
        root.addView(hostInput)

        label("ポート")
        portInput = EditText(this).apply {
            inputType = InputType.TYPE_CLASS_NUMBER
            setText(prefs.port.toString())
        }
        root.addView(portInput)

        label("IPP パス")
        pathInput = EditText(this).apply { setText(prefs.path) }
        root.addView(pathInput)

        root.addView(Button(this).apply {
            text = "保存"
            layoutParams = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            ).apply { topMargin = pad }
            setOnClickListener { save() }
        })

        setContentView(root)
    }

    private fun save() {
        val prefs = Prefs(this)
        val host = hostInput.text.toString().trim()
        val port = portInput.text.toString().trim().toIntOrNull()
        val path = pathInput.text.toString().trim()
        if (host.isEmpty() || port == null || port !in 1..65535 || !path.startsWith("/")) {
            Toast.makeText(this, "入力値が不正です", Toast.LENGTH_SHORT).show()
            return
        }
        prefs.host = host
        prefs.port = port
        prefs.path = path
        Toast.makeText(this, "保存しました", Toast.LENGTH_SHORT).show()
        finish()
    }
}
