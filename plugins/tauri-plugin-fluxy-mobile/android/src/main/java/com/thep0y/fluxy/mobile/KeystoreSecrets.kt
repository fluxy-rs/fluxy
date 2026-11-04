package com.thep0y.fluxy.mobile

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Identity secrets backed by the Android Keystore.
 *
 * The key never leaves the keystore; only the ciphertext and its IV are written
 * to a file inside the app sandbox. Losing the keystore entry (for example after
 * an uninstall, which drops the app keys) makes the file unreadable, which is the
 * behaviour we want: the identity is gone with the app.
 */
class KeystoreSecrets(private val file: File) {
    companion object {
        private const val KEY_ALIAS = "fluxy.identity.v1"
        private const val PROVIDER = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val TAG_BITS = 128
        private const val IV_BYTES = 12
        private const val HEADER_BYTES = 1
    }

    private fun secretKey(): SecretKey {
        val keyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }
        val existing = keyStore.getEntry(KEY_ALIAS, null)
        if (existing is KeyStore.SecretKeyEntry) {
            return existing.secretKey
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        return generator.generateKey()
    }

    /** @return the stored secret, or null when nothing has been stored yet. */
    fun read(): String? {
        if (!file.exists()) return null
        val payload = file.readBytes()
        if (payload.size <= HEADER_BYTES + IV_BYTES) {
            throw IllegalStateException("the secret file is truncated")
        }
        val version = payload[0].toInt()
        require(version == 1) { "unsupported secret file version: $version" }
        val iv = payload.copyOfRange(HEADER_BYTES, HEADER_BYTES + IV_BYTES)
        val ciphertext = payload.copyOfRange(HEADER_BYTES + IV_BYTES, payload.size)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(TAG_BITS, iv))
        return String(cipher.doFinal(ciphertext), Charsets.UTF_8)
    }

    fun write(value: String) {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val ciphertext = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        val payload = ByteArray(HEADER_BYTES + cipher.iv.size + ciphertext.size)
        payload[0] = 1
        System.arraycopy(cipher.iv, 0, payload, HEADER_BYTES, cipher.iv.size)
        System.arraycopy(
            ciphertext, 0, payload, HEADER_BYTES + cipher.iv.size, ciphertext.size,
        )
        file.parentFile?.mkdirs()
        val temporary = File(file.parentFile, file.name + ".tmp")
        temporary.writeBytes(payload)
        if (!temporary.renameTo(file)) {
            temporary.copyTo(file, overwrite = true)
            temporary.delete()
        }
    }

    fun delete() {
        file.delete()
        val keyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }
        if (keyStore.containsAlias(KEY_ALIAS)) {
            keyStore.deleteEntry(KEY_ALIAS)
        }
    }

    fun available(): Boolean = runCatching { secretKey() }.isSuccess

    /**
     * Write the decrypted identity to [target] when the sealed copy exists and
     * the plain file does not. Rust reads it synchronously during startup and
     * seals it again once the app is running, so the plain file only lives for
     * the first moments of a launch.
     */
    fun materializeIdentity(target: File): Boolean {
        if (!file.exists() || target.exists()) return false
        val value = read() ?: return false
        target.writeText(value, Charsets.UTF_8)
        return true
    }
}

/** Unused import guard: Base64 stays available for future binary secrets. */
internal fun keepBase64(value: ByteArray): String = Base64.encodeToString(value, Base64.NO_WRAP)
