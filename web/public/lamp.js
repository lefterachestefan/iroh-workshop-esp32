// The lamp page, for step 5: provision a lamp over Bluetooth, then switch it,
// put it in automatic mode, and set how dark it has to get.
//
// rememberKey: keep the endpoint id (or ticket) across reloads.
import { find_device } from "./wasm/workshop_web.js";
import { startNode, remember, showError, clearError } from "./node.js";

export async function lampPage({ rememberKey }) {
  const $ = (sel) => document.querySelector(sel);
  const $ssid = $("#ssid");
  const $password = $("#password");
  const $name = $("#name");
  const $provision = $("#provision");
  const $setupResult = $("#setup-result");
  const $connectForm = $("#connect-form");
  const $id = $("#endpoint-id");
  const $connect = $("#connect");
  const $connected = $("#connected");
  const $peer = $("#peer");
  const $change = $("#change");
  const $lampCard = $("#lamp-card");
  const $light = $("#light");
  const $toggle = $("#toggle");
  const $auto = $("#auto");
  const $levelFill = $("#level-fill");
  const $thresholdMark = $("#threshold-mark");
  const $level = $("#level");
  const $threshold = $("#threshold");
  const $slider = $("#threshold-slider");
  const $result = $("#result");
  const $status = $("#status");

  remember($id, rememberKey);
  // The network name, not the password.
  remember($ssid, rememberKey && `${rememberKey}:ssid`);
  let lamp;
  let state;
  // Bumped on every (re)connect, so callbacks from an old connection, like a
  // watch that ends later, leave the page alone.
  let generation = 0;

  const setSwitch = ($switch, on) => {
    $switch.setAttribute("aria-checked", String(on));
    $switch.textContent = on ? "on" : "off";
  };

  const show = (next) => {
    state = next;
    $light.classList.toggle("on", state.on);
    setSwitch($toggle, state.on);
    setSwitch($auto, state.auto);
    $levelFill.style.width = `${state.level}%`;
    $thresholdMark.style.left = `${state.threshold}%`;
    $level.textContent = state.level;
    $threshold.textContent = state.threshold;
    // Don't move the slider under the user's finger.
    if (document.activeElement !== $slider) $slider.value = state.threshold;
  };

  // Either the connect form, or "connected to …" with the lamp.
  const showConnected = (id) => {
    $peer.textContent = id ? `${id.slice(0, 10)}…` : "";
    $peer.title = id ?? "";
    $connectForm.hidden = Boolean(id);
    $connected.hidden = !id;
    $lampCard.hidden = !id;
  };

  const disconnect = () => {
    generation += 1;
    lamp = undefined;
    showConnected(undefined);
    $connect.disabled = false;
  };

  const connect = async () => {
    clearError($result);
    $connect.disabled = true;
    const current = ++generation;
    const id = $id.value.trim();
    try {
      $result.textContent = "connecting…";
      const client = node.light_v3(id);
      const first = await client.get();
      if (current !== generation) return;
      lamp = client;
      show(first);
      $result.textContent = "";
      showConnected(id);
      // Runs until the device hangs up; updates on every change, including
      // the button and the sensor.
      client.watch((next) => current === generation && show(next)).then(
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
    } catch (err) {
      if (current !== generation) return;
      $connect.disabled = false;
      showError($result, err);
    }
  };

  const node = await startNode($status);
  $connect.disabled = false;
  $provision.disabled = !navigator.bluetooth;
  if (!navigator.bluetooth) {
    $setupResult.textContent = "no Web Bluetooth in this browser";
  }

  $provision.addEventListener("click", async () => {
    clearError($setupResult);
    if (!$ssid.value.trim()) {
      showError($setupResult, "which network should it join?");
      $ssid.focus();
      return;
    }
    $provision.disabled = true;
    let device;
    try {
      device = await find_device();
      $setupResult.textContent = `found ${device.kind || "a device"} ${device.endpoint_id.slice(0, 10)}… (check against the serial console)`;
      const ticket = await device.provision(
        $ssid.value.trim(),
        $password.value,
        $name.value.trim() || device.kind,
        (detail) => {
          $setupResult.textContent = `${device.endpoint_id.slice(0, 10)}…: ${detail}`;
        },
      );
      $setupResult.textContent = "provisioned! connecting over iroh…";
      // The ticket holds the endpoint id; keep it, and connect.
      $id.value = ticket;
      $id.dispatchEvent(new Event("input"));
      disconnect();
      await connect();
      $setupResult.textContent = "";
    } catch (err) {
      showError($setupResult, err);
      device?.disconnect();
    } finally {
      $provision.disabled = false;
    }
  });

  $connect.addEventListener("click", connect);

  $change.addEventListener("click", () => {
    clearError($result);
    disconnect();
    $id.focus();
  });

  // Run a request; the device answers with the new state.
  const request = async (button, send) => {
    clearError($result);
    if (button) button.disabled = true;
    try {
      show(await send());
    } catch (err) {
      showError($result, err);
    } finally {
      if (button) button.disabled = false;
    }
  };

  $toggle.addEventListener("click", () => request($toggle, () => lamp.set(!state.on)));
  $auto.addEventListener("click", () => request($auto, () => lamp.set_auto(!state.auto)));
  // While dragging, move the mark; on release, tell the device.
  $slider.addEventListener("input", () => {
    $thresholdMark.style.left = `${$slider.value}%`;
    $threshold.textContent = $slider.value;
  });
  $slider.addEventListener("change", () =>
    request(null, () => lamp.set_threshold(Number($slider.value))),
  );
}
