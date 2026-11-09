// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

package app.justread.share

import android.app.Activity
import android.content.ClipData
import android.content.Intent
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

@TauriPlugin
class SharePlugin(private val activity: Activity) : Plugin(activity) {

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
            val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.justshare.files", file)

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
}
