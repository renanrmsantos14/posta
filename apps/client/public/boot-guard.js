// Vigia da abertura, fora do bundle (funciona mesmo se o React não subir). A UI fica invisível até as configurações
// chegarem do núcleo (`.app[data-ready]`); se isso não acontecer em 8 s, mostra um aviso com "Recarregar".
// No Android, as etapas vão para o logcat (`adb logcat -s PostaUI`) pela ponte `window.PostaLog` (MainActivity.kt).
(function () {
  var t0 = Date.now();
  function log(msg) {
    try { if (window.PostaLog) window.PostaLog.log(msg + " (" + (Date.now() - t0) + " ms)"); } catch (e) {}
  }
  window.postaBootLog = log;
  log("ui: html carregado");
  // Erros que impedem a montagem (bundle quebrado, promessa rejeitada) vão para o logcat; sem isso a tela branca não diz o porquê.
  window.addEventListener("error", function (e) { log("ui: erro: " + (e.message || (e.target && e.target.src) || "recurso")); }, true);
  window.addEventListener("unhandledrejection", function (e) { log("ui: promessa rejeitada: " + (e.reason && e.reason.message || e.reason)); });

  var box = null;
  function show() {
    box = document.createElement("div");
    box.setAttribute("role", "alert");
    box.style.cssText = "position:fixed;inset:0;z-index:2147483647;display:flex;flex-direction:column;align-items:center;" +
      "justify-content:center;gap:12px;padding:24px;text-align:center;font:15px/1.45 system-ui,sans-serif;" +
      "background:var(--bg,#fbfbfa);color:var(--text,#2f3437)";
    box.innerHTML = "<strong style='font-size:17px'>O Posta está demorando para abrir</strong>" +
      "<span style='max-width:320px;color:var(--muted,#65645f)'>Toque em Recarregar. Se continuar, feche o app " +
      "pela tela de apps recentes e abra de novo.</span>" +
      "<button type='button' style='margin-top:4px;padding:10px 20px;border:0;border-radius:8px;font:inherit;" +
      "font-weight:600;background:var(--blue,#1f6c9f);color:var(--on-blue,#fff)'>Recarregar</button>";
    box.querySelector("button").onclick = function () {
      log("ui: recarregar tocado");
      location.reload();
    };
    document.body.appendChild(box);
    log("ui: aviso de demora exibido; app montado: " + !!document.querySelector(".app"));
  }

  var timer = setInterval(function () {
    if (document.querySelector(".app[data-ready]")) {
      clearInterval(timer);
      if (box) box.remove();
      log(box ? "ui: pronta depois do aviso" : "ui: pronta");
      return;
    }
    if (!box && Date.now() - t0 >= 8000) show();
  }, 250);
})();
