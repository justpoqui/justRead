// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

package app.justread.device

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.view.WindowManager
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

/** Our own provider subclass: a unique class name keeps manifest merging conflict-free. */
class ShareFileProvider : FileProvider()

@InvokeArg
class ShareArgs {
    lateinit var title: String
    lateinit var text: String
    lateinit var fileName: String
    lateinit var mimeType: String
}

@InvokeArg
class ScreenArgs {
    var keepOn: Boolean = false
    var dim: Boolean = false
}

@InvokeArg
class ReleaseArgs {
    lateinit var repo: String
}

@InvokeArg
class InstallArgs {
    lateinit var repo: String
    lateinit var apkUrl: String
    lateinit var sha256Url: String
}

/** GitHub "owner/name". */
private val REPO_PATTERN = Regex("^[A-Za-z0-9-]+/[A-Za-z0-9._-]+$")

/** Low brightness used while the reader is dimmed (0..1 of the panel's range). */
private const val DIM_BRIGHTNESS = 0.02f

@TauriPlugin
class DevicePlugin(private val activity: Activity) : Plugin(activity) {

    /** Share sheet for exports (Phase 7). */
    @Command
    fun share(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(ShareArgs::class.java)

            // STEP 1: write the export to a private cache folder (old exports are removed).
            val dir = File(activity.cacheDir, "justshare").apply { mkdirs() }
            dir.listFiles()?.forEach { it.delete() }
            val safeName = args.fileName.replace(Regex("[/\\\\\\u0000]"), "_")
            val file = File(dir, safeName)
            file.writeText(args.text, Charsets.UTF_8)

            // STEP 2: a content:// URI the receiving app may read (and nothing else).
            val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.justdevice.files", file)

            // STEP 3: one ACTION_SEND with the file and the text; the chooser lists every app that takes either.
            val send = Intent(Intent.ACTION_SEND).apply {
                type = args.mimeType
                putExtra(Intent.EXTRA_SUBJECT, args.title)
                putExtra(Intent.EXTRA_TITLE, args.title)
                putExtra(Intent.EXTRA_TEXT, args.text)
                putExtra(Intent.EXTRA_STREAM, uri)
                clipData = ClipData.newRawUri(safeName, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            activity.startActivity(Intent.createChooser(send, args.title))
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject(e.message ?: "Could not open the share sheet")
        }
    }

    /**
     * Reader screen timeout: keep the screen on while reading (the device's
     * own timeout would otherwise interrupt), and dim the real backlight.
     * Both only affect this app's window and reset when it is left.
     */
    @Command
    fun screen(invoke: Invoke) {
        val args = invoke.parseArgs(ScreenArgs::class.java)
        // Window flags and attributes must be changed on the UI thread.
        activity.runOnUiThread {
            val window = activity.window
            if (args.keepOn) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            val attrs = window.attributes
            attrs.screenBrightness =
                if (args.dim) DIM_BRIGHTNESS else WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE
            window.attributes = attrs
        }
        invoke.resolve()
    }

    // ----- updates ------------------------------------------------------------
    // The network part of the updater lives here so it uses Android's own TLS
    // and certificate store. Both commands only ever talk to the repository's
    // own GitHub release URLs; the Rust side decides what to install.

    /** The latest GitHub release of `repo`, as the API's JSON text. */
    @Command
    fun latestRelease(invoke: Invoke) {
        val args = invoke.parseArgs(ReleaseArgs::class.java)
        if (!REPO_PATTERN.matches(args.repo)) return invoke.reject("bad repository name")
        Thread {
            try {
                val text = get(URL("https://api.github.com/repos/${args.repo}/releases/latest"))
                    .bufferedReader(Charsets.UTF_8).use { it.readText() }
                val result = JSObject()
                result.put("json", text)
                invoke.resolve(result)
            } catch (e: Exception) {
                invoke.reject(e.message ?: "Could not reach GitHub")
            }
        }.start()
    }

    /**
     * Download the release APK, check it against the published SHA-256, and
     * open Android's installer (which also checks it is signed with the same
     * key as this app). The person confirms the install; it is never silent.
     */
    @Command
    fun downloadAndInstall(invoke: Invoke) {
        val args = invoke.parseArgs(InstallArgs::class.java)
        val prefix = "https://github.com/${args.repo}/releases/download/"
        // Same rule as the Rust side: this repository's release downloads, nothing that could lead elsewhere.
        fun ok(u: String) = u.startsWith(prefix) && !u.contains("..") && !u.lowercase().contains("%2e") &&
            u.none { it == '?' || it == '#' || it == '\\' || it == '@' }
        if (!REPO_PATTERN.matches(args.repo) || !ok(args.apkUrl) || !ok(args.sha256Url)) {
            return invoke.reject("refusing a download from outside the app's releases")
        }
        // STEP 1: Android 8+: the person must allow this app to install updates (once).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !activity.packageManager.canRequestPackageInstalls()) {
            activity.startActivity(
                Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${activity.packageName}"))
            )
            return invoke.reject("permission")
        }
        Thread {
            try {
                // STEP 2: the expected checksum (first word of the .sha256 file).
                val expected = get(URL(args.sha256Url)).bufferedReader().use { it.readText() }
                    .trim().split(Regex("\\s+")).first().lowercase()
                if (!Regex("^[0-9a-f]{64}$").matches(expected)) throw Exception("bad checksum file")

                // STEP 3: download, hashing as we go.
                val dir = File(activity.cacheDir, "updates").apply { mkdirs() }
                dir.listFiles()?.forEach { it.delete() }
                val apk = File(dir, "justRead-update.apk")
                val digest = MessageDigest.getInstance("SHA-256")
                get(URL(args.apkUrl)).use { input ->
                    apk.outputStream().use { out ->
                        val buf = ByteArray(64 * 1024)
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            digest.update(buf, 0, n)
                            out.write(buf, 0, n)
                        }
                    }
                }
                val actual = digest.digest().joinToString("") { "%02x".format(it) }
                if (actual != expected) {
                    apk.delete()
                    throw Exception("The download was damaged (checksum mismatch). Try again.")
                }

                // STEP 4: hand it to the system installer.
                val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.justdevice.files", apk)
                val intent = Intent(Intent.ACTION_VIEW).apply {
                    setDataAndType(uri, "application/vnd.android.package-archive")
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                activity.runOnUiThread { activity.startActivity(intent) }
                invoke.resolve()
            } catch (e: Exception) {
                invoke.reject(e.message ?: "Download failed")
            }
        }.start()
    }

    /** HTTPS GET (redirects followed; GitHub requires a User-Agent). */
    private fun get(url: URL): java.io.InputStream {
        if (url.protocol != "https") throw Exception("https only")
        val conn = url.openConnection() as HttpURLConnection
        conn.instanceFollowRedirects = true
        conn.connectTimeout = 15_000
        conn.readTimeout = 30_000
        conn.setRequestProperty("User-Agent", "justRead-updater")
        conn.setRequestProperty("Accept", "application/vnd.github+json")
        if (conn.responseCode !in 200..299) throw Exception("GitHub answered ${conn.responseCode}")
        return conn.inputStream
    }
}
