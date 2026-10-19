import { mount } from "svelte";
import App from "./App.svelte";
// After App, so the shared rules come after the components' own, as they
// did when they lived in App's stylesheet.
import "./app.css";

export default mount(App, { target: document.getElementById("app") });
