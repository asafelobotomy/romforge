import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { VARIANT_ATTRIBUTE, watchTokens } from "./vendor/uq-tokens/uq-tokens.js";

// uQ's theme (uq-tokens, vendored unchanged in vendor/uq-tokens): the
// backend reads uQ's tokens.json (`uq_tokens`) and says when it changes
// ("uq-tokens-changed"); uq-tokens.js checks every value and sets them on
// <html>, or removes them all without uQ or with a broken file. `variant`
// is uQ's "dark" or "light" while its tokens apply, else null.
export const uq = $state({ variant: null });

export function followUq() {
  const root = document.documentElement;
  const seen = () => (uq.variant = root.getAttribute(VARIANT_ATTRIBUTE));
  new MutationObserver(seen).observe(root, { attributes: true, attributeFilter: [VARIANT_ATTRIBUTE] });
  watchTokens({
    read: () => invoke("uq_tokens"),
    subscribe: (cb) => listen("uq-tokens-changed", cb),
  });
}
