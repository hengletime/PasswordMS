package com.passwordms.app

import android.os.Bundle
import android.view.ViewGroup
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // D-41：enableEdgeToEdge 后 WebView 直接铺满屏幕，系统状态栏（时间/电量）会压住前端顶栏，
    // 顶栏左上角的汉堡按钮点击热区被状态栏抢走。wry 0.57 不处理 window insets，
    // 故在内容根视图上打 systemBars padding，把 WebView 推到系统条之内。
    // 返回 insets 不消费：键盘（IME）insets 继续下传给 WebView。
    val content = findViewById<ViewGroup>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      insets
    }
  }
}
