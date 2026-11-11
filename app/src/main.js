import { mount } from "svelte";
import App from "./App.svelte";
// After App, so the shared rules come after the components' own, as they
// did when they lived in App's stylesheet.
import "./app.css";
// uQ's theme, when romforge runs on uQ (uq.svelte.js); its fallbacks are
// romforge's own colours.
import "./vendor/uq-tokens/uq-tokens.css";
import { followUq } from "./uq.svelte.js";

followUq();

export default mount(App, { target: document.getElementById("app") });
