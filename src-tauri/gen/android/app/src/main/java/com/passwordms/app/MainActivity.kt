package com.passwordms.app

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import android.view.ViewGroup
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  /**
   * D-131：持有 WiFi 多播接收锁。
   *
   * 为什么必须有：Android 默认**丢弃**收到的广播/多播包，除非持有 MulticastLock。
   * 局域网同步的地址发现（`crates/passwordms-core/src/sync/discovery.rs`）正是靠
   * UDP 广播 255.255.255.255:47823 找到对端 —— 没有这个锁，手机根本收不到电脑的
   * 「你在哪」，于是电脑永远找不回手机换掉的新 IP，`sync_now` 只能报
   * `err.network_unreachable`。这正是一台设备换网络后「怎么都同步不上」的成因。
   *
   * 生命周期：只在 `onResume`~`onPause` 之间持有。锁本身会让无线芯片不休眠接收多播，
   * 后台常驻是白耗电；而同步只在解锁使用时才有意义。
   */
  private var multicastLock: WifiManager.MulticastLock? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    // 截屏/录屏/最近任务缩略图防护：FLAG_SECURE 必须在 super.onCreate 之前设置，
    // 否则 Activity 首帧可能以无防护状态渲染。
    window.setFlags(
      WindowManager.LayoutParams.FLAG_SECURE,
      WindowManager.LayoutParams.FLAG_SECURE
    )
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

  override fun onResume() {
    super.onResume()
    acquireMulticastLock()
  }

  override fun onPause() {
    releaseMulticastLock()
    super.onPause()
  }

  /** 幂等；取不到 WifiManager（无 WiFi 硬件/服务异常）时静默跳过——发现只是增强路径。 */
  private fun acquireMulticastLock() {
    if (multicastLock != null) {
      return
    }
    val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
    if (wifi == null) {
      return
    }
    multicastLock = wifi.createMulticastLock("passwordms-lan-discovery").also {
      // 非引用计数：避免重复 acquire 后 release 一次仍持有。
      it.setReferenceCounted(false)
      it.acquire()
    }
  }

  private fun releaseMulticastLock() {
    multicastLock?.let {
      if (it.isHeld) {
        it.release()
      }
    }
    multicastLock = null
  }
}
