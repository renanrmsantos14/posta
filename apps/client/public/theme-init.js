// Aplica o tema antes do primeiro paint (carregado síncrono no <head>), a partir do espelho em localStorage
// gravado por themes.ts. As configurações de verdade só chegam do núcleo depois; o App reconcilia então.
(function () {
  var root = document.documentElement;
  try {
    var a = JSON.parse(localStorage.getItem("posta.appearance") || "{}");
    var pref = typeof a.theme === "string" ? a.theme : "system";
    var dark = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
    var id = pref === "system" ? (dark ? "dark" : "light") : pref;
    root.dataset.theme = id;
    if (a.accent && a.accent.theme === id) {
      root.style.setProperty("--blue", a.accent.blue);
      root.style.setProperty("--blue-bg", a.accent.blueBg);
      root.style.setProperty("--on-blue", a.accent.onBlue);
    }
  } catch (e) {
    root.dataset.theme = "light";
  }
})();
