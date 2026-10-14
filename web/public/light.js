// The light switch page, for steps 3 and 4.
//
// version: 1 (step 3) or 2 (step 4, adds watching the light).
// rememberKey: keep the endpoint id across reloads (the device keeps it too).
import { startNode, remember, showError, clearError } from "./node.js";

export async function lightPage({ version, rememberKey }) {
  const $ = (sel) => document.querySelector(sel);
  const $connectForm = $("#connect-form");
  const $id = $("#endpoint-id");
  const $connect = $("#connect");
  const $connected = $("#connected");
  const $peer = $("#peer");
  const $change = $("#change");
  const $lightCard = $("#light-card");
  const $light = $("#light");
  const $toggle = $("#toggle");
  const $result = $("#result");
  const $status = $("#status");

  remember($id, rememberKey);
  let light;
  let on = false;
  // Bumped on every (re)connect, so callbacks from an old connection, like a
  // watch that ends later, leave the page alone.
  let generation = 0;

  const show = (state) => {
    on = state;
    $light.classList.toggle("on", on);
    $toggle.setAttribute("aria-checked", String(on));
    $toggle.textContent = on ? "on" : "off";
  };

  // Either the connect form, or "connected to …" with the light.
  const showConnected = (id) => {
    $peer.textContent = id ? `${id.slice(0, 10)}…` : "";
    $peer.title = id ?? "";
    $connectForm.hidden = Boolean(id);
    $connected.hidden = !id;
    $lightCard.hidden = !id;
  };

  const disconnect = () => {
    generation += 1;
    light = undefined;
    showConnected(undefined);
    $connect.disabled = false;
    $id.focus();
  };

  const node = await startNode($status);
  $connect.disabled = false;

  $connect.addEventListener("click", async () => {
    clearError($result);
    $connect.disabled = true;
    const current = ++generation;
    const id = $id.value.trim();
    try {
      $result.textContent = "connecting…";
      const client = version === 1 ? node.light_v1(id) : node.light_v2(id);
      const state = await client.get();
      if (current !== generation) return;
      light = client;
      show(state);
      $result.textContent = "";
      showConnected(id);
      if (version === 2) {
        // Runs until the device hangs up; updates on every change, including
        // button presses.
        client.watch((state) => current === generation && show(state)).then(
          () => {
            if (current !== generation) return;
            disconnect();
            showError($result, "the device hung up");
          },
          (err) => {
            if (current !== generation) return;
            disconnect();
            showError($result, err);
          },
        );
      }
    } catch (err) {
      if (current !== generation) return;
      $connect.disabled = false;
      showError($result, err);
    }
  });

  $change.addEventListener("click", () => {
    clearError($result);
    disconnect();
  });

  $toggle.addEventListener("click", async () => {
    clearError($result);
    $toggle.disabled = true;
    try {
      show(await light.set(!on));
    } catch (err) {
      showError($result, err);
    } finally {
      $toggle.disabled = false;
    }
  });
}
