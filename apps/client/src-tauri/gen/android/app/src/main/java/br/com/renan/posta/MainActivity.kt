package br.com.renan.posta

import android.content.Intent
import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject

class MainActivity : TauriActivity() {
  private var webView: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // O tao registra o ndk-context aqui dentro; o Worker não pode registrar o dele ao mesmo tempo (assert derruba o app).
    synchronized(SyncWorker.CONTEXT_LOCK) { super.onCreate(savedInstanceState) }
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
    webView.addJavascriptInterface(SafeArea, "PostaInsets")
    // Edge-to-edge: a WebView do Android devolve 0 em env(safe-area-inset-*), então a UI recebe as barras do sistema
    // (e o teclado, embaixo) em px CSS por `window.PostaInsets.get()` e pelo evento "posta-insets".
    ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      val d = view.resources.displayMetrics.density
      SafeArea.json = JSONObject()
        .put("top", bars.top / d).put("right", bars.right / d)
        .put("bottom", maxOf(bars.bottom, ime.bottom) / d).put("left", bars.left / d)
        .toString()
      view.post { (view as WebView).evaluateJavascript("window.dispatchEvent(new Event('posta-insets'))", null) }
      insets
    }
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

/** Margens das barras do sistema (e do teclado, em `bottom`) em px CSS; lidas pela UI (`src/safeArea.ts`). */
object SafeArea {
  @Volatile var json: String = ""

  @JavascriptInterface
  fun get(): String = json
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
