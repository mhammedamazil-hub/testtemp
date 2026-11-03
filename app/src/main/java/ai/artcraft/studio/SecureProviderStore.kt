package ai.artcraft.studio

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import java.net.URI

/** Stores provider API secrets encrypted with an Android Keystore AES-GCM key. */
internal class SecureProviderStore(context: Context) {
    private val prefs = context.getSharedPreferences("ai-provider-config", Context.MODE_PRIVATE)
    private val keyPrefs = context.getSharedPreferences("ai-provider-secrets", Context.MODE_PRIVATE)
    private val keyAlias = "artcraft.ai.provider.aes.v1"

    fun get(provider: String): ProviderConfig = ProviderConfig(
        endpoint = prefs.getString("$provider.endpoint", defaultEndpoint(provider)).orEmpty(),
        model = prefs.getString("$provider.model", defaultModel(provider)).orEmpty(),
        apiKey = decrypt(keyPrefs.getString(provider, null))
    )

    fun save(provider: String, endpoint: String, model: String, apiKey: String) {
        require(provider in PROVIDERS)
        val endpointValue = endpoint.trim()
        val uri = runCatching { URI(endpointValue.replace("{model}", "model")) }.getOrNull()
        require(uri != null && uri.scheme.equals("https", ignoreCase = true) && !uri.host.isNullOrBlank() && uri.userInfo == null) { "Provider endpoint must be a valid HTTPS URL without embedded credentials." }
        require(endpointValue.length <= 1_000 && !Regex("(?i)(?:^|[?&])(?:key|api[_-]?key|token|access[_-]?token|authorization)=").containsMatchIn(endpointValue)) { "Keep credentials in the encrypted API key field, not in the endpoint URL." }
        require(model.isNotBlank() && model.length <= 200) { "Model is required and must be at most 200 characters." }
        require(apiKey.length <= 512 && apiKey.none { it.code < 0x20 }) { "API key is invalid or too long." }
        prefs.edit().putString("$provider.endpoint", endpointValue).putString("$provider.model", model.trim()).apply()
        if (apiKey.isNotBlank()) keyPrefs.edit().putString(provider, encrypt(apiKey.trim())).apply()
    }

    fun clearKey(provider: String) {
        keyPrefs.edit().remove(provider).apply()
    }

    fun activeProvider(): String = prefs.getString("active.provider", "gemini")
        ?.takeIf { it in PROVIDERS } ?: "gemini"

    fun setActiveProvider(provider: String) {
        require(provider in PROVIDERS)
        prefs.edit().putString("active.provider", provider).apply()
    }

    private fun encrypt(value: String): String {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val encrypted = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        return Base64.encodeToString(cipher.iv + encrypted, Base64.NO_WRAP)
    }

    private fun decrypt(value: String?): String {
        if (value.isNullOrBlank()) return ""
        return runCatching {
            val packed = Base64.decode(value, Base64.NO_WRAP)
            val iv = packed.copyOfRange(0, 12)
            val ciphertext = packed.copyOfRange(12, packed.size)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(128, iv))
            String(cipher.doFinal(ciphertext), Charsets.UTF_8)
        }.getOrDefault("")
    }

    private fun secretKey(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(keyAlias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(keyAlias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .build()
        )
        return generator.generateKey()
    }

    companion object {
        val PROVIDERS = listOf("gemini", "anthropic", "openai")
        fun defaultEndpoint(provider: String): String = when (provider) {
            "gemini" -> "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
            "anthropic" -> "https://api.anthropic.com/v1/messages"
            else -> "https://api.openai.com/v1/chat/completions"
        }
        fun defaultModel(provider: String): String = ""
    }
}

internal data class ProviderConfig(val endpoint: String, val model: String, val apiKey: String)
