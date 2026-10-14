// Shared by the pages: load the wasm module and start the browser's iroh
// endpoint once.
import init, { Node } from "./wasm/workshop_web.js";

export async function startNode(status) {
  status.textContent = "starting iroh…";
  await init();
  const node = await Node.spawn();
  status.textContent = "iroh ready (in the browser, reaching devices over a relay)";
  return node;
}

/**
 * Remember an input's value in localStorage under `key`, if given. Trimmed,
 * unless `trim` is false: spaces at the ends of a password are part of it.
 */
export function remember(input, key, { trim = true } = {}) {
  if (!key) return;
  try {
    input.value = localStorage.getItem(key) ?? "";
    input.addEventListener("input", () =>
      localStorage.setItem(key, trim ? input.value.trim() : input.value),
    );
  } catch {}
}

export function showError(element, err) {
  element.textContent = err.message ?? `${err}`;
  element.classList.add("error");
  console.error(err);
}

export function clearError(element) {
  element.textContent = "";
  element.classList.remove("error");
}
