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

    /** 引入 `ACCESS_LOCAL_NETWORK` 的 API 级别（Android 17）。与 `LanPermissionPlugin` 同值。 */
    private const val MIN_SDK_FOR_PERM = 37

    /** 上次主动申请的时间戳；跨进程冷启动至少间隔 24h，避免每次进前台都弹窗。 */
    private const val PREF_LAN_PERM_ASKED_AT = "lan_perm_asked_at"
    private const val REASK_INTERVAL_MS = 24L * 60 * 60 * 1000

    /** 与 `LanPermissionPlugin` 共用（跨类字面值，改一处就得改两处）。 */
    const val PREF_NAME = "passwordms"

    /**
     * D-149：给 `LanPermissionPlugin` 用的「主动申请」通道（`null` = 宿主还没建好）。
     *
     * 为什么必须由 MainActivity 持有、而不能让插件自己申请：带回调的
     * `requestPermissions(Array<String>) { }` 是 **ComponentActivity 的扩展**，
     * 而 `Plugin` 构造拿到的是裸 `Activity`，调不到那个重载；裸 `Activity.requestPermissions`
     * 只有 `(Array<String>, Int)` 这种无回调形态，插件也收不到结果。
     *
     * 于是把「调用点」留在 MainActivity（这里已验证可编译），插件只负责转交一个回调。
     * 申请结果**不缓存**在此处：`status` 每次现查 `checkSelfPermission`，这是唯一的真相源。
     */
    @Volatile
    var lanPermRequester: ((cb: (Boolean) -> Unit) -> Unit)? = null
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
    // D-149：登记给插件的申请通道。必须在 onCreate 里（Activity 已 attach、还没 STARTED），
    // 这样「同步页点授予」时 requester 一定非空。
    lanPermRequester = { cb -> requestLanPermission(cb) }
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
    if (Build.VERSION.SDK_INT < MIN_SDK_FOR_PERM) {
      return
    }
    if (checkSelfPermission(PERM_LOCAL_NETWORK) == PackageManager.PERMISSION_GRANTED) {
      Log.i(TAG, "$PERM_LOCAL_NETWORK already granted")
      return
    }
    val prefs = getSharedPreferences(PREF_NAME, Context.MODE_PRIVATE)
    val lastAskedAt = prefs.getLong(PREF_LAN_PERM_ASKED_AT, 0L)
    val now = System.currentTimeMillis()
    if (lastAskedAt != 0L && now - lastAskedAt < REASK_INTERVAL_MS) {
      Log.i(TAG, "$PERM_LOCAL_NETWORK not granted; skipped (asked ${(now - lastAskedAt) / 1000}s ago)")
      return
    }
    Log.i(TAG, "requesting $PERM_LOCAL_NETWORK (sdk=${Build.VERSION.SDK_INT})")
    requestLanPermission { granted ->
      Log.i(TAG, "$PERM_LOCAL_NETWORK request result granted=$granted")
    }
  }

  /**
   * D-149：主动申请一次局域网权限，**不受 24h 节流约束**。
   *
   * 与 `ensureLocalNetworkPermission` 的区别只有节流：那条是「进前台时顺手问一次」，
   * 用户可能根本没在同步；这条由用户在同步页点「授予权限」触发，意图明确，必须真弹窗
   * ——否则用户点了按钮却什么都没发生，比不给按钮更糟。
   *
   * 副作用：仍然记下时间戳，让下一次自动申请继续遵守间隔（避免前脚刚点完、后脚切回来又问）。
   */
  private fun requestLanPermission(cb: (Boolean) -> Unit) {
    if (Build.VERSION.SDK_INT < MIN_SDK_FOR_PERM) {
      cb(true)
      return
    }
    if (checkSelfPermission(PERM_LOCAL_NETWORK) == PackageManager.PERMISSION_GRANTED) {
      cb(true)
      return
    }
    getSharedPreferences(PREF_NAME, Context.MODE_PRIVATE).edit()
      .putLong(PREF_LAN_PERM_ASKED_AT, System.currentTimeMillis())
      .apply()
    // 回调形参是 `Boolean?`（系统可在 Activity 已销毁时给 null，此时**不是授权**）。
    // 直接 `cb(granted)` 编不过；而 `granted == true` 正好把 null 归为「未授予」——
    // 这正是我们要的语义：拿不准就当作没给，绝不让警示提前消失。
    requestPermissions(arrayOf(PERM_LOCAL_NETWORK)) { granted -> cb(granted == true) }
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
