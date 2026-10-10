"use strict";
(async () => {
  const box = document.createElement("main");
  box.id = "operator-login";
  box.className = "operator-login";
  const title = document.createElement("h1");
  title.textContent = "水晶球 · 本机管理登录";
  const note = document.createElement("p");
  note.textContent =
    "请使用服务生成的本机管理链接进入，或填写管理凭据。数字人的会话凭据无法用于管理登录。";
  const command = document.createElement("code");
  command.textContent = "agent-research-lab --config local.toml serve-link";
  const form = document.createElement("form"),
    input = document.createElement("input"),
    button = document.createElement("button"),
    status = document.createElement("p");
  input.type = "password";
  input.autocomplete = "off";
  input.required = true;
  input.setAttribute("aria-label", "本机管理凭据");
  button.textContent = "进入观察台";
  status.setAttribute("role", "status");
  form.append(input, button);
  box.append(title, note, command, form, status);
  document.body.prepend(box);
  let initialized = false;
  async function start() {
    if (initialized) return;
    initialized = true;
    box.remove();
    document.body.classList.remove("operator-pending");
    for (const path of [
      "/app.js",
      "/people.js",
      "/crystal.js",
      "/lineage.js",
    ]) {
      await new Promise((resolve, reject) => {
        const script = document.createElement("script");
        script.src = path;
        script.onload = resolve;
        script.onerror = reject;
        document.head.append(script);
      });
    }
    await loadPeople();
    if (data) render();
  }
  async function login(token) {
    const response = await fetch("/api/operator/session", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Crystal-Intent": "operator-login",
      },
      body: JSON.stringify({ token }),
      cache: "no-store",
    });
    if (!response.ok) throw Error("登录未成功，请使用本机最新的管理链接。");
    input.value = "";
    await start();
  }
  form.onsubmit = async (event) => {
    event.preventDefault();
    try {
      await login(input.value);
    } catch (e) {
      status.textContent = e.message;
    }
  };
  try {
    if (location.hash.startsWith("#access=")) {
      const token = new URLSearchParams(location.hash.slice(1)).get("access");
      history.replaceState(
        null,
        "",
        location.pathname + location.search + "#collaboration",
      );
      await login(token);
    } else {
      const response = await fetch("/api/operator/session", {
        cache: "no-store",
      });
      if (response.ok) await start();
    }
  } catch (e) {
    status.textContent = e.message;
  }
})();
