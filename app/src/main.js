import { createApp } from "vue";
import "@quasar/extras/material-symbols-outlined/material-symbols-outlined.css";
import { Notify, Quasar } from "quasar";
import iconSet from "quasar/icon-set/material-symbols-outlined";
import "quasar/src/css/index.sass";
import "./styles.css";
import App from "./App.vue";

createApp(App)
  .use(Quasar, { plugins: { Notify }, config: { dark: "auto" }, iconSet })
  .mount("#app");
