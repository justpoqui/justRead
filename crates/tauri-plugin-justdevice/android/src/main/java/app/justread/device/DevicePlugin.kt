// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

package app.justread.device

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.view.WindowManager
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

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
}
