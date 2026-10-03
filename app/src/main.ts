import { createApp } from "vue";
import App from "./App.vue";
import { api, errorText, logLine, setLoggingActive } from "./api";
import "./styles.css";

window.addEventListener("error", (e) => logLine("error", `page error: ${e.message} at ${e.filename}:${e.lineno}`));
window.addEventListener("unhandledrejection", (e) => logLine("error", `unhandled: ${errorText(e.reason)}`));

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
