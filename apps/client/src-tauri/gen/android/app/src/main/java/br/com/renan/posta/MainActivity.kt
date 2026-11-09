package br.com.renan.posta

import android.content.Intent
import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import org.json.JSONObject

class MainActivity : TauriActivity() {
  private var webView: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    SyncWorker.schedule(applicationContext)
    // Girar a tela recria a Activity com o mesmo Intent: só o toque novo (sem estado salvo) abre o email.
    if (savedInstanceState == null) NotificationOpen.take(intent)
  }

  /** O toque numa notificação com o app já aberto (singleTask) chega aqui. */
  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    if (NotificationOpen.take(intent)) {
      webView?.post { webView?.evaluateJavascript("window.dispatchEvent(new Event('posta-open'))", null) }
    }
  }

  /** Ponte `window.PostaNative`: a UI busca o pedido de abertura ao carregar (cobre o app iniciado pela notificação). */
  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    this.webView = webView
    webView.addJavascriptInterface(NotificationOpen, "PostaNative")
  }

  override fun onStart() {
    super.onStart()
    SyncWorker.appVisible = true
  }

  override fun onStop() {
    SyncWorker.appVisible = false
    super.onStop()
  }
}

/** Destino do toque numa notificação de email novo: conta, pasta e, se for um só email, o id local. */
object NotificationOpen {
  private const val ACCOUNT = "posta_open_account"
  private const val FOLDER = "posta_open_folder"
  private const val ID = "posta_open_id"

  @Volatile private var pending: String? = null

  /** Grava o destino no Intent da notificação; `id` nulo abre só a pasta. */
  fun put(intent: Intent, account: String, folder: String, id: Long?) {
    intent.putExtra(ACCOUNT, account).putExtra(FOLDER, folder)
    if (id != null) intent.putExtra(ID, id)
  }

  /** Guarda o destino do Intent (e o consome, para a Activity recriada não reabrir). True se havia um. */
  fun take(intent: Intent?): Boolean {
    val account = intent?.getStringExtra(ACCOUNT)?.takeIf { it.isNotEmpty() } ?: return false
    val json = JSONObject().put("account", account).put("folder", intent.getStringExtra(FOLDER) ?: "inbox")
    if (intent.hasExtra(ID)) json.put("id", intent.getLongExtra(ID, 0))
    for (key in listOf(ACCOUNT, FOLDER, ID)) intent.removeExtra(key)
    pending = json.toString()
    return true
  }

  /** Chamado pela UI (`window.PostaNative.takePending()`): o JSON do destino, uma vez; vazio se não há. */
  @JavascriptInterface
  fun takePending(): String = pending.also { pending = null } ?: ""
}
