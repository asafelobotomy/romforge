import { mount } from "svelte";
import Radial from "./Radial.svelte";

// The overlay window's page: the radial menu over a running game.
export default mount(Radial, { target: document.getElementById("radial") });
