package jp.flll.fakeprinter

import android.content.Context
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.StateListDrawable
import android.util.TypedValue
import android.widget.Button
import android.widget.TextView

/**
 * LDSG トークンへのアクセサと共通ビュー生成。
 * 直値 #hex は res/values(-night)/colors.xml にのみ存在させる。
 * 状態表現は LDSG 公式方針どおり透過度（pressed=0.5）。
 */
object Ldsg {

    fun color(c: Context, id: Int): Int = c.getColor(id)
    fun space(c: Context, id: Int): Int = c.resources.getDimensionPixelSize(id)
    private fun fontPx(c: Context, id: Int): Float = c.resources.getDimension(id)

    /** 見出し: title 系 + Bold（LDSG 必須規約） */
    fun title(c: Context, value: String, sizeRes: Int = R.dimen.ldsg_fs_title_l): TextView =
        TextView(c).apply {
            text = value
            setTextSize(TypedValue.COMPLEX_UNIT_PX, fontPx(c, sizeRes))
            setTextColor(color(c, R.color.ldsg_text))
            typeface = Typeface.DEFAULT_BOLD
        }

    /** 本文: text 系 Regular、行送り 1.5 */
    fun text(
        c: Context,
        value: String,
        sizeRes: Int = R.dimen.ldsg_fs_text_m,
        colorRes: Int = R.color.ldsg_text,
    ): TextView = TextView(c).apply {
        text = value
        setTextSize(TypedValue.COMPLEX_UNIT_PX, fontPx(c, sizeRes))
        setTextColor(color(c, colorRes))
        setLineSpacing(0f, 1.5f)
    }

    /** カード背景（surface-raised + radius-m） */
    fun cardBackground(c: Context, colorRes: Int = R.color.ldsg_surface_raised): GradientDrawable =
        GradientDrawable().apply {
            cornerRadius = space(c, R.dimen.ldsg_radius_m).toFloat()
            setColor(color(c, colorRes))
        }

    private fun roundedRect(c: Context, fill: Int, alpha: Int = 255): GradientDrawable =
        GradientDrawable().apply {
            cornerRadius = space(c, R.dimen.ldsg_radius_m).toFloat()
            setColor(fill)
            this.alpha = alpha
        }

    private fun pressable(c: Context, fill: Int): Drawable = StateListDrawable().apply {
        addState(intArrayOf(android.R.attr.state_pressed), roundedRect(c, fill, 128)) // 0.5
        addState(intArrayOf(), roundedRect(c, fill))
    }

    /**
     * プライマリボタン: brand-primary 地。
     * LDSG コントラスト規約により Green 上は text-m(15sp) 以上 + Bold の White のみ。
     */
    fun primaryButton(c: Context, label: String): Button = Button(c).apply {
        text = label
        isAllCaps = false
        background = pressable(c, color(c, R.color.ldsg_brand_primary))
        setTextColor(color(c, R.color.ldsg_on_brand))
        setTextSize(TypedValue.COMPLEX_UNIT_PX, fontPx(c, R.dimen.ldsg_fs_text_m))
        typeface = Typeface.DEFAULT_BOLD
        stateListAnimator = null
        val padH = space(c, R.dimen.ldsg_space_5)
        val padV = space(c, R.dimen.ldsg_space_3)
        setPadding(padH, padV, padH, padV)
    }

    /** セカンダリボタン: surface-raised 地 + 通常テキスト色 */
    fun secondaryButton(c: Context, label: String): Button = Button(c).apply {
        text = label
        isAllCaps = false
        background = pressable(c, color(c, R.color.ldsg_surface_raised))
        setTextColor(color(c, R.color.ldsg_text))
        setTextSize(TypedValue.COMPLEX_UNIT_PX, fontPx(c, R.dimen.ldsg_fs_text_m))
        typeface = Typeface.DEFAULT_BOLD
        stateListAnimator = null
        val padH = space(c, R.dimen.ldsg_space_4)
        val padV = space(c, R.dimen.ldsg_space_3)
        setPadding(padH, padV, padH, padV)
    }
}
