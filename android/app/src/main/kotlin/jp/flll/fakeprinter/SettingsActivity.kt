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

/** Printer name / listen port settings (restart the printer to apply). */
class SettingsActivity : Activity() {

    private lateinit var nameInput: EditText
    private lateinit var portInput: EditText

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val prefs = Prefs(this)
        val pad = (16 * resources.displayMetrics.density).toInt()

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
        }

        fun label(text: String) = root.addView(TextView(this).apply { this.text = text })

        label("プリンタ名（他の端末に表示される名前）")
        nameInput = EditText(this).apply { setText(prefs.printerName) }
        root.addView(nameInput)

        label("待受ポート")
        portInput = EditText(this).apply {
            inputType = InputType.TYPE_CLASS_NUMBER
            setText(prefs.port.toString())
        }
        root.addView(portInput)

        label("※ 変更はプリンタの停止→開始で反映されます")

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
        val name = nameInput.text.toString().trim()
        val port = portInput.text.toString().trim().toIntOrNull()
        if (name.isEmpty() || port == null || port !in 1024..65535) {
            Toast.makeText(this, "入力値が不正です（ポートは1024〜65535）", Toast.LENGTH_SHORT).show()
            return
        }
        prefs.printerName = name
        prefs.port = port
        Toast.makeText(this, "保存しました", Toast.LENGTH_SHORT).show()
        finish()
    }
}
