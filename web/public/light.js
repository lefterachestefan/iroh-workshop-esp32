// The light switch page, for steps 2 to 4.
//
// version: 1 (steps 2, 3) or 2 (step 4, adds watching the light).
// rememberKey: keep the endpoint id across reloads (from step 3 on, the device
// keeps it too).
import { startNode, remember, showError, clearError } from "./node.js";

export async function lightPage({ version, rememberKey }) {
  const $ = (sel) => document.querySelector(sel);
  const $id = $("#endpoint-id");
  const $connect = $("#connect");
  const $on = $("#on");
  const $off = $("#off");
  const $light = $("#light");
  const $result = $("#result");
  const $status = $("#status");

  remember($id, rememberKey);
  let light;

  const show = (on) => {
    $light.classList.toggle("on", on);
    $result.textContent = on ? "on" : "off";
  };
  const enable = (yes) => {
    $on.disabled = !yes;
    $off.disabled = !yes;
  };

  const node = await startNode($status);
  $connect.disabled = false;

  $connect.addEventListener("click", async () => {
    clearError($result);
    enable(false);
    try {
      const id = $id.value.trim();
      light = version === 1 ? node.light_v1(id) : node.light_v2(id);
      $result.textContent = "connecting…";
      show(await light.get());
      enable(true);
      if (version === 2) {
        // Runs until the device hangs up; updates on every change, including
        // button presses.
        light.watch(show).then(
          () => ($status.textContent = "the device stopped sending updates"),
          (err) => showError($result, err),
        );
      }
    } catch (err) {
      showError($result, err);
    }
  });

  for (const [button, on] of [[$on, true], [$off, false]]) {
    button.addEventListener("click", async () => {
      clearError($result);
      try {
        show(await light.set(on));
      } catch (err) {
        showError($result, err);
      }
    });
  }
}
