package com.passwordms.app

import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * 局域网访问权限状态（D-149）。
 *
 * ## 为什么需要这个插件
 *
 * `MainActivity.ensureLocalNetworkPermission()` 自 D-141 起就一直在申请
 * `ACCESS_LOCAL_NETWORK`，但**结果只写进 logcat**。前端什么都拿不到，于是手机端
 * 同步失败时只能给一句「网络不可达」——而真因是权限没批，用户怎么查 WiFi、
 * 关防火墙、重启路由都没用。这个插件把那个状态摆到界面上。
 *
 * ## 只读 + 一次主动申请，没有别的
 *
 * 两个命令都很窄：
 * - `status`：现查 `checkSelfPermission`（**不缓存**，用户可能刚从系统设置里关掉）；
 * - `request`：转交给 `MainActivity.lanPermRequester`，因为它手里才有带回调的
 *   `requestPermissions` 重载（`Plugin` 拿到的是裸 `Activity`，见该类注释）。
 *
 * ## 与 MainActivity 的三处字面值耦合（改动必须同步）
 *
 * | 项 | 值 |
 * |---|---|
 * | 权限名 | `android.permission.ACCESS_LOCAL_NETWORK` |
 * | 版本闸门 | `Build.VERSION.SDK_INT >= 37`（Android 17） |
 * | 前置条件 | `MainActivity.lanPermRequester` 已登记 |
 *
 * 守门测试 `d149-lan-perm-guard.test.ts` 逐字比对，不同步会红。
 */
@TauriPlugin
class LanPermissionPlugin(private val activity: Activity) : Plugin(activity) {

    /** `Plugin.activity` 是 private，子类自己留一份（构造函数由 Tauri 以 `(Activity)` 调用）。 */
    private val host: Activity = activity

    companion object {
        private const val TAG = "PasswordMS"

        /** 与 `MainActivity.PERM_LOCAL_NETWORK` 同一字符串。 */
        private const val PERM_LOCAL_NETWORK = "android.permission.ACCESS_LOCAL_NETWORK"

        /** 与 `MainActivity.PERM_LAN_SDK` 同一闸门：Android 17（API 37）才引入此权限。 */
        private const val MIN_SDK_FOR_PERM = 37
    }

    // ------------------------------------------------------------------ 命令

    /**
     * 当前状态。
     *
     * `supported == false` 时 `granted` 恒为 true——低版本系统**根本没有**这个权限，
     * 前端据此整块不渲染。「未授予」在 Android 16 上是个不存在的问题，报出来只会
     * 让用户去系统设置里找一个永远找不到的开关。
     */
    @Command
    fun status(invoke: Invoke) {
        val supported = Build.VERSION.SDK_INT >= MIN_SDK_FOR_PERM
        val out = JSObject()
        out.put("supported", supported)
        out.put("granted", if (supported) grantedNow() else true)
        invoke.resolve(out)
    }

    /**
     * 主动申请一次（由用户点「授予权限」触发，不受 24h 节流约束）。
     *
     * **必须立即返回、把 Invoke 交给回调用**：`run_command` 把 JNI 调用投递到主线程，
     * 这里的 `@Command` 就跑在主线程上。若在此阻塞等系统对话框，窗口永远画不出来
     * （BiometricPlugin 纪律 1 的同一个坑）。
     *
     * 已授予 / 系统不支持时直接回 true，不弹空对话框。
     */
    @Command
    fun request(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < MIN_SDK_FOR_PERM) {
            resolveGranted(invoke, true)
            return
        }
        if (grantedNow()) {
            resolveGranted(invoke, true)
            return
        }
        val requester = MainActivity.lanPermRequester
        if (requester == null) {
            // 宿主还没 attach（极早期调用）。**不能假装成功**：前端会据此收起警示，
            // 然后同步继续超时，而用户刚点过「授予」——比不给按钮更难查。
            Log.w(TAG, "lanPermRequester not registered; host not ready")
            invoke.reject("host activity not ready", "platform")
            return
        }
        // 切主线程：requestPermissions 只能在主线程调，且对话框要挂在当前 Activity 上。
        host.runOnUiThread {
            try {
                requester { granted -> resolveGranted(invoke, granted) }
            } catch (e: Exception) {
                // 极端情况（Activity 已 finishing）：如实上报，让前端留住警示。
                invoke.reject("request failed: ${e.javaClass.simpleName} ${e.message}", "platform")
            }
        }
    }

    // ------------------------------------------------------------------ 内部

    private fun grantedNow(): Boolean =
        host.checkSelfPermission(PERM_LOCAL_NETWORK) == PackageManager.PERMISSION_GRANTED

    private fun resolveGranted(invoke: Invoke, granted: Boolean) {
        val out = JSObject()
        out.put("granted", granted)
        invoke.resolve(out)
    }
}
