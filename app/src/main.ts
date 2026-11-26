import { createApp } from "vue";
import App from "./App.vue";
import { api, errorText, logLine, setLoggingActive } from "./api";
import "./styles.css";

window.addEventListener("error", (e) => logLine("error", `page error: ${e.message} at ${e.filename}:${e.lineno}`));
window.addEventListener("unhandledrejection", (e) => logLine("error", `unhandled: ${errorText(e.reason)}`));

// The webview's own menu (Back, Reload, Print…) makes no sense in a desktop app.
// Text fields keep it for cut and paste.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement | null;
  if (!t?.closest("input, textarea, [contenteditable]")) e.preventDefault();
});

const app = createApp(App);
app.config.errorHandler = (e, _instance, info) => {
  logLine("error", `Vue error in ${info}: ${errorText(e)}`);
  console.error(e);
};
// Mount once the log is known to be on or off, so the first commands are in it too.
api
  .loggingState()
  .then((s) => setLoggingActive(s.enabled))
  .catch(() => {})
  .finally(() => app.mount("#app"));
