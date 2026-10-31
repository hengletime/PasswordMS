package com.passwordms.app

import android.content.Context
import android.content.pm.PackageManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.ViewGroup
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  companion object {
    private const val TAG = "PasswordMS"

    /**
     * D-141：Android 17（API 37）新增的局域网访问运行时权限。
     *
     * 为什么必须有：`targetSdk = 37`（本工程自第一个提交起如此）在 Android 17 上
     * 默认屏蔽本地网络访问 —— TCP 连接、UDP 单播/多播/广播全覆盖。系统对本 UID 的
     * AppOps 恒为 `ignore`，包被**静默丢弃**（手机侧 `ss -tne` 可见 SYN-SENT 卡死、
     * 无 RST），表现为「配对/同步永远超时」，而浏览器、输入法等低 targetSdk 应用
     * 照常能连——这正是「同一台手机同一 WiFi，只有本应用不通」的成因。
     *
     * 授权方式：运行时弹窗（NEARBY_DEVICES 组）。用户在系统设置「附近的设备」里
     * 也可手动开关。低版本系统（< 37）无此权限，直接跳过，不弹无意义对话框。
     */
    private const val PERM_LOCAL_NETWORK = "android.permission.ACCESS_LOCAL_NETWORK"

    /** 上次主动申请的时间戳；跨进程冷启动至少间隔 24h，避免每次进前台都弹窗。 */
    private const val PREF_LAN_PERM_ASKED_AT = "lan_perm_asked_at"
    private const val REASK_INTERVAL_MS = 24L * 60 * 60 * 1000
  }

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
    ensureLocalNetworkPermission()
  }

  override fun onPause() {
    releaseMulticastLock()
    super.onPause()
  }

  /**
   * D-141：确保局域网运行时权限已授予。
   *
   * 幂等；已授予时零开销。被拒绝后按 24h 间隔再问（Android 自身在「拒绝两次」后
   * 不再弹窗，此时只能由用户去系统设置里手动打开——日志会记录状态便于排障）。
   */
  private fun ensureLocalNetworkPermission() {
    // 低版本系统没有这个权限，声明会被忽略，也不需要申请。
    if (Build.VERSION.SDK_INT < 37) {
      return
    }
    if (checkSelfPermission(PERM_LOCAL_NETWORK) == PackageManager.PERMISSION_GRANTED) {
      Log.i(TAG, "$PERM_LOCAL_NETWORK already granted")
      return
    }
    val prefs = getSharedPreferences("passwordms", Context.MODE_PRIVATE)
    val lastAskedAt = prefs.getLong(PREF_LAN_PERM_ASKED_AT, 0L)
    val now = System.currentTimeMillis()
    if (lastAskedAt != 0L && now - lastAskedAt < REASK_INTERVAL_MS) {
      Log.i(TAG, "$PERM_LOCAL_NETWORK not granted; skipped (asked ${(now - lastAskedAt) / 1000}s ago)")
      return
    }
    prefs.edit().putLong(PREF_LAN_PERM_ASKED_AT, now).apply()
    Log.i(TAG, "requesting $PERM_LOCAL_NETWORK (sdk=${Build.VERSION.SDK_INT})")
    requestPermissions(arrayOf(PERM_LOCAL_NETWORK)) { granted ->
      Log.i(TAG, "$PERM_LOCAL_NETWORK request result granted=$granted")
    }
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
