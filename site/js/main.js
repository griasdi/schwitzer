// Downloads, copy buttons, the agents terminal and the sessions ticker.
import { formatRuntime, formatTokens, startDrops } from "./stage.js";

const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
const RELEASES = "https://api.github.com/repos/griasdi/schwitzer/releases/latest";

// ---------- Platform and downloads ----------

async function detectPlatform() {
  const ua = navigator.userAgentData;
  const p = (ua?.platform || navigator.userAgent || "").toLowerCase();
  if (p.includes("android") || /iphone|ipad|ipod/.test(p)) return { os: "other" };
  if (p.includes("win")) return { os: "windows" };
  if (p.includes("mac")) return { os: "macos" };
  if (p.includes("linux") || p.includes("x11")) {
    let arch = /aarch64|arm64/.test(navigator.userAgent.toLowerCase()) ? "aarch64" : "x86_64";
    if (ua?.getHighEntropyValues) {
      try {
        const v = await ua.getHighEntropyValues(["architecture"]);
        if (v.architecture) arch = v.architecture === "arm" ? "aarch64" : "x86_64";
      } catch {
        // keep the user agent's guess
      }
    }
    return { os: "linux", arch };
  }
  return { os: "other" };
}

function heroChoice({ os, arch }) {
  switch (os) {
    case "linux": return { label: "Download for Linux", meta: `AppImage ${arch}`, asset: `linux-${arch}.AppImage` };
    case "macos": return { label: "Download for macOS", meta: "Apple silicon · tar.gz", asset: "macos-aarch64.tar.gz" };
    case "windows": return { label: "Download for Windows", meta: "Installer · x86_64", asset: "windows-x86_64-setup.exe" };
    default: return { label: "Download for desktop", meta: "Linux, macOS and Windows", asset: null };
  }
}

async function setupDownloads() {
  const platform = await detectPlatform();
  const choice = heroChoice(platform);
  const cta = document.getElementById("cta-download");
  const meta = document.getElementById("cta-meta");
  document.getElementById("cta-label").textContent = choice.label;
  meta.textContent = choice.asset ? `Latest release · ${choice.meta}` : choice.meta;
  if (choice.asset) cta.dataset.asset = choice.asset;
  else cta.href = "#download";

  const yours = document.querySelector(`.platform[data-platform="${platform.os}"]`);
  if (yours) {
    yours.classList.add("is-yours");
    let first = yours.querySelector("[data-asset]");
    if (platform.os === "linux") first = yours.querySelector(`[data-asset="linux-${platform.arch}.AppImage"]`) ?? first;
    first.classList.replace("btn-secondary", "btn-primary");
  }

  try {
    const res = await fetch(RELEASES, { headers: { Accept: "application/vnd.github+json" } });
    if (!res.ok) return;
    const release = await res.json();
    const assets = Array.isArray(release.assets) ? release.assets : [];
    for (const link of document.querySelectorAll("[data-asset]")) {
      const asset = assets.find((a) => typeof a.name === "string" && a.name.endsWith(`-${link.dataset.asset}`));
      if (asset?.browser_download_url) link.href = asset.browser_download_url;
    }
    if (typeof release.tag_name === "string" && release.tag_name) {
      document.getElementById("release-version").textContent = release.tag_name;
      if (choice.asset && cta.dataset.asset && cta.href.includes("/download/")) meta.textContent = `${release.tag_name} · ${choice.meta}`;
    }
  } catch {
    // offline or rate-limited: the static links to the latest release stay
  }
}

// ---------- Copy buttons ----------

function setupCopy() {
  for (const btn of document.querySelectorAll("[data-copy]")) {
    const label = btn.querySelector("[data-label]");
    btn.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(btn.dataset.copy);
        label.textContent = "Copied";
        clearTimeout(btn._t);
        btn._t = setTimeout(() => { label.textContent = "Copy"; }, 2000);
      } catch {
        const box = btn.parentElement.querySelector(".mono-box");
        const range = document.createRange();
        range.selectNodeContents(box);
        const sel = getSelection();
        sel.removeAllRanges();
        sel.addRange(range);
        label.textContent = "Selected, press Ctrl+C";
      }
    });
  }
}

// ---------- Visibility helper ----------

function watchVisible(node, onChange, threshold = 0.15) {
  new IntersectionObserver(([e]) => onChange(e.isIntersecting), { threshold }).observe(node);
}

// ---------- Agents terminal ----------

function setupAgentsTerminal() {
  const term = document.getElementById("term-agents");
  if (reducedMotion) return;
  const body = term.querySelector(".term-body");
  const lines = [...body.children];
  for (const l of lines) l.hidden = true;
  let visible = false;
  let started = false;
  const wait = async (ms) => {
    let left = ms;
    while (left > 0) {
      await new Promise((r) => setTimeout(r, 50));
      if (visible && !document.hidden) left -= 50;
    }
  };
  async function type(line) {
    const text = line.textContent;
    line.textContent = "";
    line.hidden = false;
    const caret = document.createElement("span");
    caret.className = "term-caret";
    const node = document.createTextNode("");
    line.append(node, caret);
    for (let i = 1; i <= text.length; i++) {
      node.data = text.slice(0, i);
      await wait(28);
    }
    await wait(350);
    caret.remove();
  }
  async function run() {
    await wait(300);
    for (const line of lines) {
      if (line.classList.contains("is-cmd")) await type(line);
      else { line.hidden = false; await wait(260); }
    }
  }
  watchVisible(term, (v) => {
    visible = v;
    if (v && !started) { started = true; run(); }
  }, 0.4);
}

// ---------- Sessions ticker ----------

function setupSessions() {
  const panel = document.getElementById("sessions-panel");
  let visible = false;
  watchVisible(panel, (v) => { visible = v; });
  for (const s of panel.querySelectorAll("[data-ticking]")) s.classList.add("is-active");
  startDrops(panel, ".session.is-active", () => visible && !document.hidden);
  if (reducedMotion) return;
  setInterval(() => {
    if (!visible || document.hidden) return;
    for (const s of panel.querySelectorAll("[data-ticking]")) {
      const minutes = Number(s.dataset.minutes) + 1;
      const tokens = Number(s.dataset.tokens) + 80 + Math.floor(Math.random() * 321);
      s.dataset.minutes = minutes;
      s.dataset.tokens = tokens;
      s.querySelector("[data-t]").textContent = formatRuntime(minutes);
      s.querySelector("[data-k]").textContent = formatTokens(tokens);
    }
  }, 2000);
}

setupDownloads();
setupCopy();
setupAgentsTerminal();
setupSessions();
