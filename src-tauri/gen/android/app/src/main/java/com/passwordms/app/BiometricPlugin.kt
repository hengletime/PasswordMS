package com.passwordms.app

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.provider.Settings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import java.util.concurrent.atomic.AtomicBoolean
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Android 快速解锁适配器（docs/08-biometric-unlock.md §23）。
 *
 * 与 Windows 侧的两段式（DPAPI + UserConsentVerifier）不同，这里**一段就够**：
 * Keystore 里生成的 AES 密钥标了 `setUserAuthenticationRequired(true)`，每次使用都必须
 * 先过 BiometricPrompt；而 BiometricPrompt 拿到的是 `CryptoObject(cipher)`——认证成功后
 * 解锁的正是这把 cipher 本身。**加解密和在场验证在对象层面就是同一件事**，不存在
 * 「认证过了但用另一把钥匙开」的缝隙，也不会弹两次框。
 *
 * 绑定熵 `bind` 走 GCM 的 **AAD**（`updateAAD`），语义与 Windows 的 DPAPI entropy 等价：
 * 换库、换设备、换了 VK 都解不开。
 *
 * ## 三条不能违反的纪律
 *
 * 1. **绝不在主线程等结果**。Tauri 的 `run_command` 是用 `run_on_android_context` 把 JNI
 *    调用投递到**主线程**的，也就是说本类的 `@Command` 方法跑在主线程上。一旦在这里
 *    `CountDownLatch.await()`，BiometricPrompt 的窗口就永远画不出来，直接死锁。
 *    正确做法：`@Command` 立即返回（**不 resolve**），把 Invoke 交给对话框回调，认证结束
 *    时再 `resolve` / `reject`。
 * 2. **每个 Invoke 只回一次**。对话框可能既触发 `onAuthenticationError` 又触发别的回调，
 *    用 `AtomicBoolean` 兜住，重复回复会让 Rust 侧的回调表错乱。
 * 3. **失败不折叠**。密钥因新增指纹而失效（`KeyPermanentlyInvalidatedException`）与
 *    「用户取消」是两件完全不同的事：前者要标 `invalidated` 让界面说「请重新开启」，
 *    后者必须静默。
 */
@TauriPlugin
class BiometricPlugin(private val activity: Activity) : Plugin(activity) {

    /** `Plugin.activity` 是 private，子类自己留一份（构造函数由 Tauri 以 `(Activity)` 调用）。 */
    private val host: Activity = activity

    companion object {
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"

        /** 每个保险库一把密钥：删别名即撤销，互不牵连。 */
        private const val ALIAS_PREFIX = "passwordms/bio/"

        private const val GCM_TAG_BITS = 128
        private const val IV_BYTES = 12

        /** 与 core 的 `SEAL_VK_MIN_LEN / SEAL_VK_MAX_LEN` 对齐；超界一律拒，不落盘。 */
        private const val SEALED_MIN_BYTES = IV_BYTES + 16
        private const val SEALED_MAX_BYTES = 1024
    }

    /** 同一时刻只允许一个对话框（Rust 侧还有一道 `try_begin_biometric`，这里是第二道）。 */
    private val promptInFlight = AtomicBoolean(false)

    // ------------------------------------------------------------------ 命令

    /**
     * 当前可用性。
     *
     * `BIOMETRIC_ERROR_NONE_ENROLLED` 在这里是**准确**的——Android 会把「有硬件但没录指纹」
     * 老实报成 `NONE_ENROLLED`，不像 Windows 的 `DeviceNotPresent` 那样骗人（§20.1）。
     */
    @Command
    fun availability(invoke: Invoke) {
        val out = JSObject()
        out.put("status", availabilityName())
        invoke.resolve(out)
    }

    /** 写入封印：`iv || ciphertext(+tag)`。 */
    @Command
    fun seal(invoke: Invoke) {
        val args = invoke.getArgs()
        val keyId = args.getString("key_id")
        val dataB64 = args.getString("data")
        val bindB64 = args.getString("bind")

        if (keyId.isEmpty() || dataB64.isEmpty()) {
            invoke.reject("missing arguments", "platform")
            return
        }

        val plain: ByteArray
        val bind: ByteArray
        try {
            plain = Base64.decode(dataB64, Base64.NO_WRAP)
            bind = Base64.decode(bindB64, Base64.NO_WRAP)
        } catch (e: IllegalArgumentException) {
            invoke.reject("malformed base64: ${e.message}", "platform")
            return
        }

        val alias = ALIAS_PREFIX + keyId
        val cipher = try {
            cipherForSeal(alias)
        } catch (e: KeyPermanentlyInvalidatedException) {
            // 旧密钥已随「新增 / 删除指纹」永久失效。**必须删掉重建**——不删的话
            // 「重新开启」会永远卡在这一步：钥匙已经废了，却还占着这个别名，
            // 每次 init 都抛同一个异常（用户会看到「重新开启」点了没反应）。
            deleteKey(alias)
            try {
                cipherForSeal(alias)
            } catch (e2: Exception) {
                invoke.reject("recreate key failed: ${e2.javaClass.simpleName} ${e2.message}", "platform")
                return
            }
        } catch (e: Exception) {
            invoke.reject("init encrypt failed: ${e.javaClass.simpleName} ${e.message}", "platform")
            return
        }

        authenticate(invoke, cipher) { authed ->
            val out = JSObject()
            try {
                authed.updateAAD(bind)
                val ct = authed.doFinal(plain)
                val iv = authed.iv
                if (iv == null || iv.size != IV_BYTES) {
                    throw IllegalStateException("unexpected iv length: ${iv?.size}")
                }
                val sealed = ByteArray(iv.size + ct.size)
                System.arraycopy(iv, 0, sealed, 0, iv.size)
                System.arraycopy(ct, 0, sealed, iv.size, ct.size)
                if (sealed.size > SEALED_MAX_BYTES) {
                    throw IllegalStateException("sealed too large: ${sealed.size}")
                }
                out.put("sealed", Base64.encodeToString(sealed, Base64.NO_WRAP))
                out
            } catch (e: KeyPermanentlyInvalidatedException) {
                throw InvalidationError(e)
            }
        }
    }

    /** 读封印。`bind` 若与写入时不同，GCM 校验必失败 ⇒ 映射为 `tampered`。 */
    @Command
    fun unseal(invoke: Invoke) {
        val args = invoke.getArgs()
        val keyId = args.getString("key_id")
        val sealedB64 = args.getString("sealed")
        val bindB64 = args.getString("bind")

        if (keyId.isEmpty() || sealedB64.isEmpty()) {
            invoke.reject("missing arguments", "platform")
            return
        }

        val sealed: ByteArray
        val bind: ByteArray
        try {
            sealed = Base64.decode(sealedB64, Base64.NO_WRAP)
            bind = Base64.decode(bindB64, Base64.NO_WRAP)
        } catch (e: IllegalArgumentException) {
            invoke.reject("malformed base64: ${e.message}", "platform")
            return
        }

        if (sealed.size < SEALED_MIN_BYTES || sealed.size > SEALED_MAX_BYTES) {
            invoke.reject("sealed length out of range: ${sealed.size}", "tampered")
            return
        }

        val alias = ALIAS_PREFIX + keyId
        val cipher: Cipher
        try {
            val ks = keystore()
            if (!ks.containsAlias(alias)) {
                // 密钥被清掉了（恢复出厂 / 用户在系统里删了凭据）：不是用户取消，
                // 也不能说数据坏了——按失效上报，让界面引导重开。
                invoke.reject("keystore entry missing", "invalidated")
                return
            }
            val key = (ks.getEntry(alias, null) as KeyStore.SecretKeyEntry).secretKey
            cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(GCM_TAG_BITS, sealed, 0, IV_BYTES))
        } catch (e: KeyPermanentlyInvalidatedException) {
            invoke.reject("key permanently invalidated", "invalidated")
            return
        } catch (e: android.security.keystore.UserNotAuthenticatedException) {
            // 正常情况下走不到：doFinal 才需要认证，而我们只在认证成功的回调里调它。
            // 若真发生，那是平台侧的异常（不能报 canceled —— 用户并没有取消，
            // 静默会把「按了指纹却什么都没发生」变成一次无解释的失败）。
            invoke.reject("unexpected unauthenticated cipher", "platform")
            return
        } catch (e: Exception) {
            invoke.reject("init decrypt failed: ${e.javaClass.simpleName} ${e.message}", "platform")
            return
        }

        authenticate(invoke, cipher) { authed ->
            val out = JSObject()
            try {
                authed.updateAAD(bind)
                val plain = authed.doFinal(sealed, IV_BYTES, sealed.size - IV_BYTES)
                out.put("data", Base64.encodeToString(plain, Base64.NO_WRAP))
                out
            } catch (e: javax.crypto.AEADBadTagException) {
                // 认证标签不对：要么 bind 变了（换库/换 VK），要么文件被改过。
                throw TamperedError(e)
            } catch (e: KeyPermanentlyInvalidatedException) {
                throw InvalidationError(e)
            }
        }
    }

    /** 删密钥（撤销的唯一动作）。幂等。 */
    @Command
    fun remove(invoke: Invoke) {
        val keyId = invoke.getArgs().getString("key_id")
        if (keyId.isEmpty()) {
            invoke.reject("missing key_id", "platform")
            return
        }
        try {
            deleteKey(ALIAS_PREFIX + keyId)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("delete failed: ${e.message}", "platform")
            return
        }
    }

    /**
     * 打开系统的生物识别录入页（D-114 的「给一条能走的路」在 Android 上的等价物）。
     *
     * 无参数：目标 Action 是编译期常量，Rust 侧也没有参数位，前端构造不出别的 Intent。
     */
    @Command
    fun openSettings(invoke: Invoke) {
        val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Intent(Settings.ACTION_BIOMETRIC_ENROLL)
        } else {
            // API 24–29 没有专门的生物识别录入页，退到安全设置由用户自己找。
            Intent(Settings.ACTION_SECURITY_SETTINGS)
        }
        try {
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            host.startActivity(intent)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("cannot open settings: ${e.javaClass.simpleName} ${e.message}", "platform")
        }
    }

    // ------------------------------------------------------------------ 内部

    private fun availabilityName(): String {
        val code = BiometricManager.from(host)
            .canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG)
        return when (code) {
            BiometricManager.BIOMETRIC_SUCCESS -> "available"
            BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED -> "not_configured"
            BiometricManager.BIOMETRIC_ERROR_NO_HARDWARE -> "not_present"
            // 硬件在但此刻用不了（被占用 / 传感器脏污 / 临时故障）——不可当失效处理。
            BiometricManager.BIOMETRIC_ERROR_HW_UNAVAILABLE -> "busy"
            BiometricManager.BIOMETRIC_ERROR_SECURITY_UPDATE_REQUIRED -> "unsupported"
            BiometricManager.BIOMETRIC_ERROR_UNSUPPORTED -> "unsupported"
            BiometricManager.BIOMETRIC_STATUS_UNKNOWN -> "busy"
            else -> "unsupported"
        }
    }

    private fun keystore(): KeyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }

    /**
     * 取（或新建）密钥并 init 成加密态。
     *
     * auth-per-use 密钥的 `init` 本身能过，cipher 处于「未认证」态——第一次 `doFinal`
     * 才需要认证。认证由 `authenticate()` 的 `CryptoObject` 完成，这正是
     * 「闸门与加解密合一」。
     *
     * 密钥若已因生物特征变更永久失效，`init` 会抛 `KeyPermanentlyInvalidatedException`，
     * 由调用方决定删掉重建（见 `seal`）。
     */
    private fun cipherForSeal(alias: String): Cipher {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, getOrCreateKey(alias))
        return cipher
    }

    /** 删别名（幂等）。撤销与「失效后重建」共用同一段逻辑。 */
    private fun deleteKey(alias: String) {
        val ks = keystore()
        if (ks.containsAlias(alias)) ks.deleteEntry(alias)
    }

    private fun getOrCreateKey(alias: String): SecretKey {
        val ks = keystore()
        (ks.getEntry(alias, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }

        val builder = KeyGenParameterSpec.Builder(
            alias,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            // 每次使用都要认证（不是「认证后 N 秒内有效」）——快速解锁要的就是每次在场。
            .setUserAuthenticationRequired(true)
            // 新增 / 删除指纹即让密钥永久失效。这是 Android 唯一能告给我们的「凭据变更」
            // 信号，也是 Windows 侧拿不到的东西（§21.3）。别为了体验把它关掉：那等于
            // 放弃了「有人偷加一枚指纹」这个真实威胁模型的防线。
            .setInvalidatedByBiometricEnrollment(true)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // API 30+：timeout=0 表示 auth-per-use，且限定强生物识别。
            builder.setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
        } else {
            // API 24–29：-1 = 每次使用都需认证（旧版无法进一步限定 STRONG）。
            @Suppress("DEPRECATION")
            builder.setUserAuthenticationValidityDurationSeconds(-1)
        }

        val kg = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        kg.init(builder.build())
        return kg.generateKey()
    }

    /**
     * 弹 BiometricPrompt 并在回调里完成 [action]。
     *
     * **本方法必须立即返回**：回调里的 `finish` 才是唯一回复点。任何阻塞主线程的写法
     * （`await` / `Thread.join` / 忙等）都会让对话框画不出来，见类注释纪律 1。
     */
    private fun authenticate(
        invoke: Invoke,
        cipher: Cipher,
        action: (Cipher) -> JSObject,
    ) {
        if (!promptInFlight.compareAndSet(false, true)) {
            invoke.reject("another biometric prompt is in progress", "platform")
            return
        }

        val finished = AtomicBoolean(false)
        fun finish(block: () -> Unit) {
            if (finished.compareAndSet(false, true)) {
                promptInFlight.set(false)
                block()
            }
        }

        val fragmentActivity = host as? FragmentActivity
        if (fragmentActivity == null) {
            promptInFlight.set(false)
            invoke.reject("host activity is not a FragmentActivity", "platform")
            return
        }

        val prompt = BiometricPrompt(
            fragmentActivity,
            ContextCompat.getMainExecutor(host),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val authed = result.cryptoObject?.cipher
                    if (authed == null) {
                        finish { invoke.reject("no cipher in result", "platform") }
                        return
                    }
                    finish {
                        try {
                            invoke.resolve(action(authed))
                        } catch (e: InvalidationError) {
                            invoke.reject("key permanently invalidated", "invalidated")
                        } catch (e: TamperedError) {
                            invoke.reject("aead tag mismatch", "tampered")
                        } catch (e: Exception) {
                            invoke.reject("crypto failed: ${e.javaClass.simpleName} ${e.message}", "platform")
                        }
                    }
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    finish {
                        // 用户主动取消 / 按了「取消」按钮 → canceled（前端必须静默）。
                        // 其余错误码都不是「用户不想」，按平台故障上报，避免被误判成失效
                        // 而删掉一条本来好用的封印。
                        val canceled = errorCode == BiometricPrompt.ERROR_USER_CANCELED ||
                            errorCode == BiometricPrompt.ERROR_NEGATIVE_BUTTON ||
                            errorCode == BiometricPrompt.ERROR_CANCELED
                        invoke.reject(errString.toString(), if (canceled) "canceled" else "platform")
                    }
                }

                // onAuthenticationFailed 只是「这一下没认出来」，系统会继续让试。
                // 绝不能在这里 reject——那会把「指纹没对准」变成一次失败解锁。
            }
        )

        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(host.getString(R.string.bio_prompt_title))
            .setSubtitle(host.getString(R.string.bio_prompt_subtitle))
            .setNegativeButtonText(host.getString(R.string.bio_prompt_cancel))
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
            .setConfirmationRequired(false)
            .build()

        try {
            prompt.authenticate(info, BiometricPrompt.CryptoObject(cipher))
        } catch (e: Exception) {
            finish { invoke.reject("prompt start failed: ${e.javaClass.simpleName} ${e.message}", "platform") }
        }
    }

    /** 密钥因生物特征变更 / 重置而永久失效。 */
    private class InvalidationError(cause: Throwable) : Exception(cause)

    /** GCM 标签校验失败：bind 变了或数据被改。 */
    private class TamperedError(cause: Throwable) : Exception(cause)
}
