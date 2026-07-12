package jp.flll.fakeprinter

import android.app.Activity
import android.os.Bundle
import android.text.InputType
import android.view.ViewGroup
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch
import android.widget.Toast

/** LDSG 準拠の設定画面: プリンタ名 / ポート / PC 自動転送 */
class SettingsActivity : Activity() {

    private lateinit var nameInput: EditText
    private lateinit var portInput: EditText
    private lateinit var forwardSwitch: Switch
    private lateinit var forwardHostInput: EditText
    private lateinit var forwardPortInput: EditText

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val prefs = Prefs(this)
        val s2 = Ldsg.space(this, R.dimen.ldsg_space_2)
        val s4 = Ldsg.space(this, R.dimen.ldsg_space_4)
        val s5 = Ldsg.space(this, R.dimen.ldsg_space_5)

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(s4, s4, s4, s4)
        }

        fun sectionTitle(text: String, topMargin: Int = s5) =
            root.addView(Ldsg.title(this, text, R.dimen.ldsg_fs_title_m),
                LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.WRAP_CONTENT).apply { this.topMargin = topMargin })

        fun fieldLabel(text: String) =
            root.addView(Ldsg.text(this, text, R.dimen.ldsg_fs_text_s, R.color.ldsg_text_sub),
                LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = s2 })

        root.addView(Ldsg.title(this, "設定", R.dimen.ldsg_fs_title_xl))

        sectionTitle("このプリンタ")
        fieldLabel("プリンタ名（他の端末に表示される名前）")
        nameInput = EditText(this).apply {
            setText(prefs.printerName)
            setTextColor(Ldsg.color(this@SettingsActivity, R.color.ldsg_text))
        }
        root.addView(nameInput)

        fieldLabel("待受ポート")
        portInput = EditText(this).apply {
            inputType = InputType.TYPE_CLASS_NUMBER
            setText(prefs.port.toString())
            setTextColor(Ldsg.color(this@SettingsActivity, R.color.ldsg_text))
        }
        root.addView(portInput)

        sectionTitle("PC への自動転送")
        forwardSwitch = Switch(this).apply {
            text = "受信した PDF を Windows 版 fake-printer に転送する"
            isChecked = prefs.forwardEnabled
            setTextColor(Ldsg.color(this@SettingsActivity, R.color.ldsg_text))
        }
        root.addView(forwardSwitch, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = s2 })

        fieldLabel("転送先ホスト（PC の IP）")
        forwardHostInput = EditText(this).apply {
            setText(prefs.forwardHost)
            setTextColor(Ldsg.color(this@SettingsActivity, R.color.ldsg_text))
        }
        root.addView(forwardHostInput)

        fieldLabel("転送先ポート")
        forwardPortInput = EditText(this).apply {
            inputType = InputType.TYPE_CLASS_NUMBER
            setText(prefs.forwardPort.toString())
            setTextColor(Ldsg.color(this@SettingsActivity, R.color.ldsg_text))
        }
        root.addView(forwardPortInput)

        root.addView(Ldsg.text(this, "※ 変更はプリンタの停止→開始で反映されます",
            R.dimen.ldsg_fs_text_xs, R.color.ldsg_text_tertiary),
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = s4 })

        root.addView(Ldsg.primaryButton(this, "保存").apply { setOnClickListener { save() } },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = s4 })

        setContentView(ScrollView(this).apply { addView(root) })
    }

    private fun save() {
        val prefs = Prefs(this)
        val name = nameInput.text.toString().trim()
        val port = portInput.text.toString().trim().toIntOrNull()
        val forwardHost = forwardHostInput.text.toString().trim()
        val forwardPort = forwardPortInput.text.toString().trim().toIntOrNull()

        if (name.isEmpty() || port == null || port !in 1024..65535) {
            Toast.makeText(this, "プリンタ名/ポートが不正です（ポートは1024〜65535）", Toast.LENGTH_SHORT).show()
            return
        }
        if (forwardSwitch.isChecked) {
            if (forwardHost.isEmpty() || forwardPort == null || forwardPort !in 1..65535) {
                Toast.makeText(this, "転送先が不正です", Toast.LENGTH_SHORT).show()
                return
            }
            // ループ防止: 転送先に自分自身を指定できない
            val selfIp = PrinterForegroundService.localIpv4()?.hostAddress
            if (forwardPort == port && (forwardHost == selfIp || forwardHost == "127.0.0.1" || forwardHost == "localhost")) {
                Toast.makeText(this, "転送先にこの端末自身は指定できません", Toast.LENGTH_LONG).show()
                return
            }
        }

        prefs.printerName = name
        prefs.port = port
        prefs.forwardEnabled = forwardSwitch.isChecked
        if (forwardHost.isNotEmpty()) prefs.forwardHost = forwardHost
        if (forwardPort != null) prefs.forwardPort = forwardPort
        Toast.makeText(this, "保存しました", Toast.LENGTH_SHORT).show()
        finish()
    }
}
