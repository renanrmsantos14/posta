package br.com.renan.posta

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Sincroniza a Caixa de entrada com o app fechado (a cada ~15 min, com rede) chamando o núcleo Rust
 * (`nativeSync`, em src-tauri/src/android_sync.rs) e avisa das mensagens novas. Veja docs/android.md.
 */
class SyncWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
  private external fun nativeSync(context: Context, dir: String): String

  override fun doWork(): Result {
    // Com o app aberto o IDLE já mantém tudo em dia e notifica.
    if (appVisible) return Result.success()
    val text = try {
      System.loadLibrary("posta_lib")
      // Mesma pasta que o Tauri usa como app_data_dir (identificador do app).
      val dir = File(applicationContext.dataDir, "br.com.renan.posta")
      if (!File(dir, "accounts.json").exists()) return Result.success()
      nativeSync(applicationContext, dir.absolutePath)
    } catch (e: Throwable) {
      Log.w(TAG, "falha ao chamar o núcleo Rust", e)
      return Result.retry()
    }
    if (text.startsWith("ERRO")) {
      Log.w(TAG, text)
      return Result.retry()
    }
    if (text.isNotEmpty()) notifyNew(text)
    return Result.success()
  }

  private fun notifyNew(text: String) {
    val ctx = applicationContext
    // Sem a permissão POST_NOTIFICATIONS (Android 13+) ou com notificações bloqueadas, não há o que mostrar.
    if (!NotificationManagerCompat.from(ctx).areNotificationsEnabled()) return
    val manager = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      manager.createNotificationChannel(NotificationChannel(CHANNEL, "Novas mensagens", NotificationManager.IMPORTANCE_DEFAULT))
    }
    val lines = text.split("\n")
    val total = lines[0].toIntOrNull() ?: return
    val items = lines.drop(1).filter { it.isNotBlank() }.map { it.split("\t", limit = 2) }
    val (title, body) = if (total == 1 && items.size == 1 && items[0].size == 2) {
      items[0][0] to items[0][1]
    } else {
      "Posta" to "$total mensagens novas"
    }
    // Tocar na notificação abre o app.
    val open = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)?.apply {
      flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
    }
    val pending = PendingIntent.getActivity(ctx, 0, open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    val notification = NotificationCompat.Builder(ctx, CHANNEL)
      .setSmallIcon(ctx.applicationInfo.icon)
      .setContentTitle(title)
      .setContentText(body)
      .setContentIntent(pending)
      .setAutoCancel(true)
      .build()
    manager.notify(NOTIFICATION_ID, notification)
  }

  companion object {
    private const val TAG = "PostaSync"
    private const val CHANNEL = "posta_novas"
    private const val NOTIFICATION_ID = 4101
    private const val UNIQUE = "posta-sync"

    /** MainActivity marca em onStart/onStop; o Worker roda no mesmo processo quando o app está aberto. */
    @Volatile var appVisible = false

    /** Agenda (uma vez, sem duplicar) a sincronização periódica; o mínimo do WorkManager é 15 min. */
    fun schedule(context: Context) {
      val request = PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES)
        .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
        .build()
      WorkManager.getInstance(context).enqueueUniquePeriodicWork(UNIQUE, ExistingPeriodicWorkPolicy.KEEP, request)
    }
  }
}
