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

/** Remember an input's value in localStorage under `key`, if given. */
export function remember(input, key) {
  if (!key) return;
  try {
    input.value = localStorage.getItem(key) ?? "";
    input.addEventListener("input", () => localStorage.setItem(key, input.value.trim()));
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
