// Aplica o tema antes do primeiro paint (carregado síncrono no <head>), a partir do espelho em localStorage
// gravado por themes.ts (tema, destaque, tema personalizado e estilo). As configurações de verdade só chegam do núcleo depois; o App reconcilia então.
(function () {
  var root = document.documentElement;
  try {
    var a = JSON.parse(localStorage.getItem("posta.appearance") || "{}");
    var pref = typeof a.theme === "string" ? a.theme : "system";
    var dark = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
    var custom = a.custom && a.custom.theme === pref ? a.custom : null;
    var id = custom ? custom.base : pref === "system" ? (dark ? "dark" : "light") : pref;
    root.dataset.theme = id;
    if (custom) for (var k in custom.vars) root.style.setProperty(k, custom.vars[k]);
    var attrs = { radius: "data-radius", shadow: "data-shadow", font: "data-font", railDensity: "data-rail" };
    if (a.style) for (var s in attrs) if (typeof a.style[s] === "string") root.setAttribute(attrs[s], a.style[s]);
    if (a.accent && a.accent.theme === id) {
      root.style.setProperty("--blue", a.accent.blue);
      root.style.setProperty("--blue-bg", a.accent.blueBg);
      root.style.setProperty("--on-blue", a.accent.onBlue);
    }
  } catch (e) {
    root.dataset.theme = "light";
  }
})();
