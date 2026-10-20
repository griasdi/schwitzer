// The two-desk demo: mara (you) and teo share the room "harbor". Every action leaves the actor's
// desk, crosses the room server and lands on the other desk.

const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
const EASE = "cubic-bezier(0.16, 1, 0.3, 1)";
const DROP_PATH = "M23 18 C23 18 16 27 16 31.5 A7 7 0 0 0 30 31.5 C30 27 23 18 23 18 Z";
const SVG_NS = "http://www.w3.org/2000/svg";

// ---------- Shared helpers (also used by main.js) ----------

export function formatTokens(n) {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

export function formatRuntime(minutes) {
  return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}`;
}

/** Sweat drops from the top-right corner of every host `container.querySelectorAll(selector)`
 * returns, one every 1.6–2.4 s, at most six alive per container. `active()` gates spawning. */
export function startDrops(container, selector, active) {
  if (reducedMotion.matches) return () => {};
  const next = new WeakMap();
  let alive = 0;
  const timer = setInterval(() => {
    if (!active()) return;
    const now = performance.now();
    for (const host of container.querySelectorAll(selector)) {
      const due = next.get(host);
      if (due === undefined) { next.set(host, now + 300 + Math.random() * 1500); continue; }
      if (now < due || alive >= 6) continue;
      next.set(host, now + 1600 + Math.random() * 800);
      const drop = document.createElementNS(SVG_NS, "svg");
      drop.setAttribute("viewBox", "15 17 16 22");
      drop.setAttribute("class", "drop");
      drop.setAttribute("aria-hidden", "true");
      drop.innerHTML = `<path d="${DROP_PATH}"/>`;
      drop.style.top = "-3px";
      drop.style.right = `${10 + Math.random() * 18}px`;
      host.append(drop);
      alive++;
      const anim = drop.animate(
        [{ transform: "translateY(0)", opacity: 0.7 }, { transform: "translateY(14px)", opacity: 0 }],
        { duration: 900, easing: "cubic-bezier(0.5, 0, 0.75, 0)" },
      );
      anim.onfinish = anim.oncancel = () => { drop.remove(); alive--; };
    }
  }, 200);
  return () => clearInterval(timer);
}

// ---------- Model ----------

const INITIAL = {
  tasks: [
    { n: 7, title: "Retry failed webhooks with backoff", stage: "building", claim: "teo" },
    { n: 10, title: "Export a room as Markdown", stage: "idea" },
    { n: 5, title: "Sign in with passkeys", stage: "done" },
  ],
  sessions: [{ id: "s7", who: "teo", title: 'Build Schwitzer task #7 "Retry failed webhooks with backoff"', task: 7, tokens: 41200, minutes: 28, idle: false }],
};

const STAGES = {
  idea: { group: "Ideas", label: "Idea", lamp: "l-idea", hollow: true },
  proposed: { group: "Proposals", label: "Proposed", lamp: "l-new", hollow: true, button: "accept" },
  accepted: { group: "Planning", label: "Accepted", lamp: "l-planning" },
  planning: { group: "In progress", label: "Being planned", lamp: "l-running" },
  planready: { group: "Ready to pick", label: "Plan ready", lamp: "l-pickable", button: "approve" },
  approved: { group: "Ready to pick", label: "Approved", lamp: "l-pickable", button: "pick" },
  building: { group: "In progress", label: "Building", lamp: "l-running", button: "unclaim" },
  done: { group: "Done", label: "Done", lamp: "l-closed" },
};
const LAMP_VAR = {
  "l-idea": "var(--stage-idea)", "l-new": "var(--stage-new)", "l-planning": "var(--stage-planning)",
  "l-pickable": "var(--stage-pickable)", "l-running": "var(--stage-running)", "l-closed": "var(--stage-closed)",
};
const GROUPS = ["In progress", "Ready to pick", "Planning", "Proposals", "Ideas", "Done"];
const ALWAYS = new Set(["Proposals", "Ideas"]);
const HUE = { mara: "violet", teo: "orange" };
const BUTTONS = {
  accept: { label: "Accept", icon: "i-check", kind: "btn-primary" },
  approve: { label: "Approve", icon: "i-check", kind: "btn-primary" },
  pick: { label: "Pick", icon: "i-grab", kind: "btn-primary" },
  unclaim: { label: "Unclaim", icon: "i-leave", kind: "btn-secondary" },
};

const clone = (v) => structuredClone(v);
const task = (s, n) => s.tasks.find((t) => t.n === n);
const setTask = (s, n, patch) => Object.assign(task(s, n), patch);
const session = (s, id) => s.sessions.find((x) => x.id === id);

const S12P = { id: "s12p", who: "teo", title: 'Plan Schwitzer task #12 "Rate-limit the login endpoint"', task: 12, tokens: 3100, minutes: 0, idle: false };
const S12B = { id: "s12b", who: "mara", title: 'Build Schwitzer task #12 "Rate-limit the login endpoint"', task: 12, tokens: 1800, minutes: 0, idle: false };

// Ops: ["cmd", desk, text] types a command; ["line", desk, text, tone]; ["send", from, mutate, log,
// stage] changes the room; ["wait", n, hint] waits for the visitor's press; ["notice", text];
// ["sleep", ms]; ["fork", delay, ops] runs ops alongside.
const STEPS = [
  null,
  {
    caption: "teo's agent proposes #12. It asked teo first.",
    ops: [
      ["sleep", 400],
      ["cmd", "teo", "/schwitzer propose rate-limit the login endpoint"],
      ["line", "teo", 'Drafting a proposal for project "harbor"…', "dim"],
      ["sleep", 500],
      ["line", "teo", 'Send "Rate-limit the login endpoint"? › Send', "dim"],
      ["sleep", 400],
      ["line", "teo", "schwitzer_propose ✓ #12", "ok"],
      ["send", "teo", (s) => s.tasks.push({ n: 12, title: "Rate-limit the login endpoint", stage: "proposed" }), "#12 proposed", "proposed"],
    ],
  },
  {
    caption: "You accept. Only people can.",
    ops: [
      ["wait", 12, "Your call. Press Accept."],
      ["send", "mara", (s) => setTask(s, 12, { stage: "accepted" }), "#12 accepted", "accepted"],
    ],
  },
  {
    caption: "teo's agent claims #12, plans it, hands in the plan.",
    ops: [
      ["cmd", "teo", "/schwitzer plan"],
      ["line", "teo", "#12 Rate-limit the login endpoint", ""],
      ["line", "teo", "Task #12 is claimed for planning. Press Enter to start.", "ok"],
      ["send", "teo", (s) => { setTask(s, 12, { stage: "planning", claim: "teo" }); s.sessions.push(clone(S12P)); }, "#12 claimed by teo", "planning"],
      ["sleep", 900],
      ["line", "teo", "Reading src/auth/login.rs", "dim"],
      ["sleep", 1100],
      ["line", "teo", "Plan: token bucket per client IP, 10 tries a minute, 429 with Retry-After", "dim"],
      ["sleep", 700],
      ["line", "teo", "Plan for task #12 sent to Schwitzer; it waits for approval in the app.", "ok"],
      ["send", "teo", (s) => { setTask(s, 12, { stage: "planready", claim: null }); session(s, "s12p").idle = true; }, "#12 plan ready", "planready"],
    ],
  },
  {
    caption: "You approve the plan, then pick it: the build is yours.",
    ops: [
      ["wait", 12, "Plan ready. Press Approve."],
      ["send", "mara", (s) => setTask(s, 12, { stage: "approved" }), "#12 approved", "approved"],
      ["wait", 12, "Approved. Pick it for your agent."],
      ["notice", "#12 is yours, prompt copied"],
      ["send", "mara", (s) => { setTask(s, 12, { stage: "building", claim: "mara" }); s.sessions.push(clone(S12B)); }, "#12 picked by mara", "building"],
      ["cmd", "mara", 'Work on schwitzer task #12 in room "harbor": Rate-limit the login endpoint'],
      ["line", "mara", "schwitzer-claim: 3f9a0c7e…", "dim"],
    ],
  },
  {
    caption: "Your agent builds and finishes #12. teo's finishes #7.",
    ops: [
      ["sleep", 1400],
      ["line", "mara", "Editing src/auth/login.rs", "dim"],
      ["sleep", 1000],
      ["line", "mara", "Editing src/auth/limit.rs", "dim"],
      ["sleep", 1300],
      ["line", "mara", "cargo test   48 passed", "ok"],
      ["line", "mara", "schwitzer_finish ✓ #12", "ok"],
      ["send", "mara", (s) => { setTask(s, 12, { stage: "done", claim: null }); s.sessions = s.sessions.filter((x) => x.id !== "s12b"); }, "#12 done", "done"],
      ["fork", 600, [
        ["line", "teo", "schwitzer_finish ✓ #7", "ok"],
        ["send", "teo", (s) => { setTask(s, 7, { stage: "done", claim: null }); s.sessions = s.sessions.filter((x) => x.id !== "s7"); }, "#7 done", "done"],
      ]],
    ],
  },
  { caption: "That's the loop: nobody steered anybody else's agent.", ops: [["sleep", 4000]] },
];

// ---------- DOM ----------

const stage = document.getElementById("stage");
const grid = document.getElementById("stage-grid");
const wiresSvg = document.getElementById("wires");
const roomNode = grid.querySelector(".room-node");
const roomLamp = roomNode.querySelector(".lamp");
const roomLog = roomNode.querySelector(".room-log");
const caption = document.getElementById("stage-caption");
const stepper = document.getElementById("stepper");
const toggle = document.getElementById("stage-toggle");
const terms = { mara: document.querySelector("#term-mara .term-body"), teo: document.querySelector("#term-teo .term-body") };
const termBoxes = [document.getElementById("term-mara"), document.getElementById("term-teo")];

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}
function iconEl(name) {
  const i = el("span", `icon ${name}`);
  i.setAttribute("aria-hidden", "true");
  return i;
}

class AppWindow {
  constructor(root) {
    this.root = root;
    this.me = root.dataset.me;
    this.list = root.querySelector(".win-list");
    this.sessionsEl = root.querySelector(".win-sessions");
    this.notice = root.querySelector(".win-notice");
    this.slabs = new Map();
    this.sessionEls = new Map();
    this.groups = new Map();
    this.list.replaceChildren();
    for (const name of GROUPS) {
      const g = el("div", "group");
      const head = el("div", "group-head");
      head.append(iconEl("i-chevron"), document.createTextNode(name), el("span", "pill", "0"));
      if (ALWAYS.has(name)) {
        const add = el("span", "add");
        add.append(iconEl("i-plus"));
        head.append(add);
      }
      const body = el("div", "group-body");
      g.append(head, body);
      this.list.append(g);
      this.groups.set(name, { g, head, body, count: head.querySelector(".pill") });
    }
    this.sessionsEl.replaceChildren();
    this.sessionsHead = el("div", "sessions-head");
    this.sessionsCount = el("span", "pill", "0");
    this.sessionsHead.append(document.createTextNode("Sessions"), this.sessionsCount);
    this.sessionsEl.append(this.sessionsHead);
  }

  zoom() {
    return parseFloat(getComputedStyle(this.root).zoom) || 1;
  }

  slab(t) {
    let s = this.slabs.get(t.n);
    if (s) return { s, fresh: false };
    const root = el("div", "task slab");
    root.dataset.n = t.n;
    const main = el("div", "task-main");
    const title = el("div", "task-title");
    const lamp = el("i", "lamp");
    title.append(lamp, el("span", "", t.title));
    const meta = el("div", "task-meta");
    const pill = el("span", "pill");
    const tag = el("span", "tag");
    meta.append(el("span", "num", `#${t.n}`), pill, tag);
    main.append(title, meta);
    root.append(main);
    s = { root, lamp, pill, tag, btn: null, btnKind: null, lampCls: null };
    this.slabs.set(t.n, s);
    return { s, fresh: true };
  }

  render(state, animate) {
    const z = this.zoom();
    const before = new Map();
    if (animate) {
      for (const [n, s] of this.slabs) before.set(`t${n}`, [s.root, s.root.getBoundingClientRect()]);
      for (const [name, g] of this.groups) if (!g.g.hidden) before.set(`g${name}`, [g.head, g.head.getBoundingClientRect()]);
    }
    const counts = new Map(GROUPS.map((g) => [g, 0]));
    const seen = new Set();
    const sorted = [...state.tasks].sort((a, b) => a.n - b.n);
    const pulses = [];
    const entered = [];
    for (const t of sorted) {
      const info = STAGES[t.stage];
      const { s, fresh } = this.slab(t);
      seen.add(t.n);
      counts.set(info.group, counts.get(info.group) + 1);
      const lampCls = `lamp ${info.hollow ? "lamp--hollow " : ""}${info.lamp}`;
      if (s.lampCls && s.lampCls !== lampCls) pulses.push(s.lamp);
      s.lamp.className = s.lampCls = lampCls;
      s.pill.textContent = info.label;
      s.tag.hidden = !t.claim;
      if (t.claim) { s.tag.textContent = t.claim; s.tag.className = `tag tag--${HUE[t.claim]}`; }
      s.root.classList.toggle("is-building", t.stage === "building");
      let kind = info.button ?? null;
      if (kind === "unclaim" && t.claim !== this.me) kind = null;
      if (kind !== s.btnKind) {
        s.btn?.remove();
        s.btn = null;
        if (kind) {
          const b = BUTTONS[kind];
          const btn = el("button", `btn ${b.kind}`);
          btn.type = "button";
          btn.tabIndex = -1;
          btn.setAttribute("aria-disabled", "true");
          btn.append(iconEl(b.icon), document.createTextNode(b.label));
          s.root.append(btn);
          s.btn = btn;
        }
        s.btnKind = kind;
      }
      const body = this.groups.get(info.group).body;
      body.append(s.root);
      if (fresh) entered.push(s.root);
    }
    for (const [n, s] of this.slabs) {
      if (seen.has(n)) continue;
      this.slabs.delete(n);
      s.root.remove();
    }
    for (const [name, g] of this.groups) {
      const c = counts.get(name);
      g.count.textContent = c;
      g.g.hidden = c === 0 && !ALWAYS.has(name);
    }
    if (animate) {
      for (const [key, [node, r0]] of before) {
        if (!node.isConnected) continue;
        const r1 = node.getBoundingClientRect();
        const dx = (r0.left - r1.left) / z;
        const dy = (r0.top - r1.top) / z;
        if (Math.abs(dx) < 0.5 && Math.abs(dy) < 0.5) continue;
        node.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], { duration: 420, easing: EASE });
        void key;
      }
      for (const node of entered) {
        node.animate([{ opacity: 0, transform: "scale(.96)" }, { opacity: 1, transform: "none" }], { duration: 300, easing: EASE });
      }
      for (const lamp of pulses) {
        lamp.animate([{ transform: "scale(1)" }, { transform: "scale(1.6)" }, { transform: "scale(1)" }], { duration: 220, easing: "ease-out" });
      }
    }
    this.renderSessions(state, animate);
  }

  renderSessions(state, animate) {
    const seen = new Set();
    for (const x of state.sessions) {
      seen.add(x.id);
      let s = this.sessionEls.get(x.id);
      const fresh = !s;
      if (fresh) {
        const root = el("div", "session slab");
        const main = el("div", "session-main");
        const top = el("div", "session-top");
        top.append(el("span", `tag tag--${HUE[x.who]}`, x.who), el("span", "session-title", x.title));
        const cap = el("div", "session-cap");
        const capText = el("span");
        const idle = el("span", "pill", "idle");
        cap.append(capText, idle);
        const time = el("span", "session-time");
        main.append(top, cap);
        root.append(main, time);
        s = { root, capText, idle, time };
        this.sessionEls.set(x.id, s);
        this.sessionsEl.append(root);
        if (animate) root.animate([{ opacity: 0, transform: "scale(.96)" }, { opacity: 1, transform: "none" }], { duration: 300, easing: EASE });
      }
      const tick = ticks.get(x.id) ?? x;
      s.capText.textContent = `harbor · #${x.task} · ${formatTokens(tick.tokens)}`;
      s.idle.hidden = !x.idle;
      s.root.classList.toggle("is-active", !x.idle);
      s.time.textContent = formatRuntime(tick.minutes);
    }
    for (const [id, s] of this.sessionEls) {
      if (seen.has(id)) continue;
      this.sessionEls.delete(id);
      if (animate) {
        const a = s.root.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 160, fill: "forwards" });
        a.onfinish = () => s.root.remove();
      } else s.root.remove();
    }
    this.sessionsCount.textContent = state.sessions.length;
  }
}

const wins = { mara: new AppWindow(document.getElementById("win-mara")), teo: new AppWindow(document.getElementById("win-teo")) };
const views = { mara: clone(INITIAL), teo: clone(INITIAL) };
const ticks = new Map();

// ---------- Scheduler ----------

const ABORT = Symbol("abort");
let runId = 0;
let currentStep = 1;
let userPaused = false;
let autoPaused = false;
let fastMode = reducedMotion.matches;
const flights = new Set();
const isPaused = () => userPaused || autoPaused || document.hidden;

function updatePause() {
  const p = isPaused();
  for (const a of flights) p ? a.pause() : a.play();
  stage.classList.toggle("is-paused", p);
}

async function sleep(ms, id) {
  let left = ms;
  while (true) {
    if (id !== runId) throw ABORT;
    if (left <= 0) return;
    const step = Math.min(left, 50);
    await new Promise((r) => setTimeout(r, step));
    if (!isPaused()) left -= step;
  }
}

async function fly(anim, id) {
  flights.add(anim);
  if (isPaused()) anim.pause();
  try {
    await anim.finished;
  } catch {
    // cancelled on abort
  } finally {
    flights.delete(anim);
  }
  if (id !== runId) throw ABORT;
}

// ---------- Terminals ----------

/** Clears a terminal down to its idle prompt, a `›` with a blinking caret. */
function clearTerm(desk) {
  const idle = el("p", "term-line is-cmd term-idle");
  idle.append(el("span", "term-caret"));
  terms[desk].replaceChildren(idle);
}

function termLine(desk, text, tone) {
  const p = el("p", `term-line${tone ? ` is-${tone}` : ""}`, text);
  const body = terms[desk];
  body.insertBefore(p, body.querySelector(".term-idle"));
  while (body.childElementCount > 30) body.firstElementChild.remove();
  return p;
}

async function typeCmd(desk, text, id, instant) {
  const p = termLine(desk, "", "cmd");
  if (instant) { p.textContent = text; return; }
  const idle = terms[desk].querySelector(".term-idle");
  idle.hidden = true;
  const caret = el("span", "term-caret");
  const node = document.createTextNode("");
  p.append(node, caret);
  try {
    for (let i = 1; i <= text.length; i++) {
      node.data = text.slice(0, i);
      await sleep(28, id);
    }
    await sleep(200, id);
  } finally {
    caret.remove();
    idle.hidden = false;
  }
}

// ---------- Geometry: wires and packets ----------

function rel(r) {
  const g = grid.getBoundingClientRect();
  return { l: r.left - g.left, t: r.top - g.top, r: r.right - g.left, b: r.bottom - g.top, w: r.width, h: r.height };
}
const narrow = () => matchMedia("(max-width: 899px)").matches;

function anchors() {
  const room = rel(roomNode.getBoundingClientRect());
  if (narrow()) {
    const t = rel(document.getElementById("term-teo").getBoundingClientRect());
    const m = rel(wins.mara.root.getBoundingClientRect());
    const cx = room.l + room.w / 2;
    return {
      teo: [{ x: cx, y: t.b }, { x: cx, y: room.t }],
      mara: [{ x: cx, y: m.t }, { x: cx, y: room.b }],
    };
  }
  const m = rel(wins.mara.root.getBoundingClientRect());
  const t = rel(wins.teo.root.getBoundingClientRect());
  const y = (m.t + m.b) / 2;
  return {
    mara: [{ x: m.r, y }, { x: room.l, y }],
    teo: [{ x: t.l, y }, { x: room.r, y }],
  };
}

/** Where the room node faces a desk. */
function roomPoint(desk) {
  const room = rel(roomNode.getBoundingClientRect());
  if (narrow()) return { x: room.l + room.w / 2, y: desk === "teo" ? room.t : room.b };
  return { x: desk === "mara" ? room.l : room.r, y: room.t + room.h / 2 };
}

/** The lamp of task `n`'s slab in a desk's window: the packet leaves from it in the lamp's new
 * colour. The desk's wire end when the slab is not there; in the narrow layout teo's packets
 * leave from the ticker. */
function slabPoint(desk, n) {
  const a = anchors()[desk][0];
  if (narrow() && desk === "teo") return a;
  const slab = wins[desk].slabs.get(n);
  if (!slab) return a;
  const l = rel(slab.lamp.getBoundingClientRect());
  return { x: l.l + l.w / 2, y: l.t + l.h / 2 };
}

/** Where the change lands: the sender lamp's place in the receiving window (both windows share
 * one layout). */
function landingPoint(from, n) {
  const to = from === "mara" ? "teo" : "mara";
  if (narrow()) return slabPoint(to, n);
  const slab = wins[from].slabs.get(n);
  if (!slab) return anchors()[to][0];
  const l = rel(slab.lamp.getBoundingClientRect());
  const fw = rel(wins[from].root.getBoundingClientRect());
  const tw = rel(wins[to].root.getBoundingClientRect());
  return { x: tw.l + (l.l + l.w / 2 - fw.l), y: tw.t + (l.t + l.h / 2 - fw.t) };
}

function drawWires() {
  const a = anchors();
  const g = grid.getBoundingClientRect();
  wiresSvg.setAttribute("viewBox", `0 0 ${g.width} ${g.height}`);
  wiresSvg.innerHTML = Object.values(a)
    .map(([p, q]) => `<line x1="${p.x}" y1="${p.y}" x2="${q.x}" y2="${q.y}"/>`)
    .join("");
}

function flashRoom(log) {
  roomLamp.classList.remove("is-flash");
  void roomLamp.offsetWidth;
  roomLamp.classList.add("is-flash");
  roomLog.textContent = log;
  roomLog.classList.add("is-on");
  clearTimeout(flashRoom.t);
  flashRoom.t = setTimeout(() => roomLog.classList.remove("is-on"), 1500);
}

async function hop(packet, from, to, id) {
  const anim = packet.animate(
    [{ transform: `translate(${from.x}px, ${from.y}px)` }, { transform: `translate(${to.x}px, ${to.y}px)` }],
    { duration: 480, easing: EASE, fill: "forwards" },
  );
  await fly(anim, id);
}

/** The actor's window shows the change at once; the other one when the packet arrives. */
async function send(from, mutate, log, stageKey, id, instant) {
  const to = from === "mara" ? "teo" : "mara";
  mutate(views[from]);
  for (const x of views[from].sessions) if (!ticks.has(x.id)) ticks.set(x.id, { tokens: x.tokens, minutes: x.minutes });
  if (instant) { mutate(views[to]); return; }
  wins[from].render(views[from], !fastMode);
  if (fastMode) {
    flashRoom(log);
    mutate(views[to]);
    wins[to].render(views[to], false);
    return;
  }
  const n = Number(/#(\d+)/.exec(log)?.[1]);
  const start = slabPoint(from, n);
  const packet = el("div", "packet");
  packet.style.setProperty("--lamp", LAMP_VAR[STAGES[stageKey].lamp]);
  grid.append(packet);
  try {
    await hop(packet, start, roomPoint(from), id);
    flashRoom(log);
    await sleep(160, id);
    await hop(packet, roomPoint(to), landingPoint(from, n), id);
  } finally {
    packet.remove();
  }
  mutate(views[to]);
  wins[to].render(views[to], true);
}

// ---------- Hints, ghost cursor, notice ----------

let hintBubble = null;
let ghost = null;

/** Removes every hint, bubble and ghost cursor; only the live run calls it. */
function clearHint() {
  for (const e of grid.querySelectorAll(".hint-bubble, .ghost-cursor")) e.remove();
  hintBubble = null;
  ghost = null;
  for (const b of wins.mara.root.querySelectorAll(".is-hinted")) {
    b.classList.remove("is-hinted");
    b.tabIndex = -1;
    b.setAttribute("aria-disabled", "true");
  }
}

function showHint(btn, text) {
  btn.classList.add("is-hinted");
  btn.tabIndex = 0;
  btn.removeAttribute("aria-disabled");
  hintBubble = el("div", "hint-bubble", text);
  hintBubble.setAttribute("aria-hidden", "true");
  grid.append(hintBubble);
  const r = rel(btn.getBoundingClientRect());
  const bw = hintBubble.offsetWidth;
  const left = Math.max(4, Math.min(r.r + 10 - bw, grid.clientWidth - bw - 4));
  hintBubble.style.left = `${left}px`;
  hintBubble.style.top = `${r.t - hintBubble.offsetHeight - 10}px`;
  hintBubble.style.setProperty("--tip", `${r.r - left - r.w / 2}px`);
  requestAnimationFrame(() => hintBubble?.classList.add("is-on"));
}

async function press(btn) {
  btn.classList.add("is-pressed");
  await new Promise((r) => setTimeout(r, 120));
  btn.classList.remove("is-pressed");
}

async function waitPress(n, text, id) {
  const btn = wins.mara.slabs.get(n)?.btn;
  if (!btn) return;
  showHint(btn, text);
  if (!fastMode) caption.textContent = text;
  let pressed = false;
  const onClick = () => {
    if (pressed) return;
    pressed = true;
    ghost?.remove();
    ghost = null;
    if (userPaused) setUserPaused(false);
  };
  btn.addEventListener("click", onClick);
  try {
    let waited = 0;
    while (!pressed) {
      await new Promise((r) => setTimeout(r, 50));
      if (id !== runId) throw ABORT;
      if (!isPaused() && !fastMode) waited += 50;
      if (waited >= 6000 && !ghost) {
        ghost = document.createElementNS(SVG_NS, "svg");
        ghost.setAttribute("viewBox", "0 0 16 16");
        ghost.setAttribute("class", "ghost-cursor");
        ghost.setAttribute("aria-hidden", "true");
        ghost.innerHTML = '<path d="M2 1.5 L2 13.5 L5.2 10.6 L7.4 15 L9.4 14 L7.3 9.7 L11.6 9.7 Z"/>';
        grid.append(ghost);
        const frame = rel(wins.mara.root.getBoundingClientRect());
        const r = rel(btn.getBoundingClientRect());
        const anim = ghost.animate(
          [{ transform: `translate(${frame.r - 24}px, ${frame.b - 40}px)` }, { transform: `translate(${r.l + r.w / 2 - 2}px, ${r.t + r.h / 2 - 2}px)` }],
          { duration: 700, easing: EASE, fill: "forwards" },
        );
        await fly(anim, id);
        if (ghost) pressed = true;
      }
    }
    await press(btn);
    if (id !== runId) throw ABORT;
    caption.textContent = STEPS[currentStep].caption;
  } finally {
    btn.removeEventListener("click", onClick);
    if (id === runId) clearHint();
  }
}

function showNotice(text) {
  const n = wins.mara.notice;
  n.textContent = text;
  n.classList.add("is-on");
  clearTimeout(showNotice.t);
  showNotice.t = setTimeout(() => n.classList.remove("is-on"), 4000);
}

// ---------- Steps ----------

async function runOps(ops, id, instant) {
  const forks = [];
  for (const op of ops) {
    if (id !== runId) throw ABORT;
    const [kind] = op;
    if (kind === "cmd") {
      await typeCmd(op[1], op[2], id, instant || fastMode);
      if (!instant && !fastMode) await sleep(260, id);
    } else if (kind === "line") {
      termLine(op[1], op[2], op[3]);
      if (!instant && !fastMode) await sleep(260, id);
    } else if (kind === "send") {
      const p = send(op[1], op[2], op[3], op[4], id, instant);
      if (instant || fastMode) await p;
      else forks.push(swallowAbort(p));
    } else if (kind === "wait") {
      if (!instant) await waitPress(op[1], op[2], id);
    } else if (kind === "notice") {
      if (!instant) showNotice(op[1]);
    } else if (kind === "sleep") {
      if (!instant && !fastMode) await sleep(op[1], id);
    } else if (kind === "fork") {
      if (instant || fastMode) await runOps(op[2], id, instant);
      else forks.push(swallowAbort(sleep(op[1], id).then(() => runOps(op[2], id, false))));
    }
  }
  await Promise.all(forks);
  if (id !== runId) throw ABORT;
}

function swallowAbort(p) {
  return p.catch((e) => {
    if (e !== ABORT) throw e;
  });
}

function setStep(k) {
  for (const b of stepper.querySelectorAll("button")) {
    if (Number(b.dataset.step) === k) b.setAttribute("aria-current", "step");
    else b.removeAttribute("aria-current");
  }
  currentStep = k;
  caption.textContent = STEPS[k].caption;
}

function resetModel() {
  views.mara = clone(INITIAL);
  views.teo = clone(INITIAL);
  ticks.clear();
  for (const x of INITIAL.sessions) ticks.set(x.id, { tokens: x.tokens, minutes: x.minutes });
  clearTerm("mara");
  clearTerm("teo");
  wins.mara.notice.classList.remove("is-on");
}

async function play(from, id) {
  try {
    for (let k = from; ; k++) {
      if (k > 6) {
        if (fastMode) return;
        const fading = [wins.mara.root, wins.teo.root, ...termBoxes];
        for (const e of fading) e.classList.add("is-fading");
        await sleep(220, id);
        resetModel();
        wins.mara.render(views.mara, false);
        wins.teo.render(views.teo, false);
        for (const e of fading) e.classList.remove("is-fading");
        await sleep(300, id);
        k = 1;
      }
      setStep(k);
      await runOps(STEPS[k].ops, id, false);
      if (!fastMode) await sleep(700, id);
    }
  } catch (e) {
    if (e !== ABORT) throw e;
  }
}

/** Replays steps 1…k-1 at once, then plays from k. */
async function jump(k) {
  const id = ++runId;
  for (const a of flights) a.cancel();
  flights.clear();
  for (const p of grid.querySelectorAll(".packet")) p.remove();
  clearHint();
  resetModel();
  try {
    for (let j = 1; j < k; j++) await runOps(STEPS[j].ops, id, true);
  } catch (e) {
    if (e !== ABORT) throw e;
    return;
  }
  wins.mara.render(views.mara, false);
  wins.teo.render(views.teo, false);
  play(k, id);
}

// ---------- Layout, ticking, visibility ----------

function layout() {
  for (const w of Object.values(wins)) {
    const frame = w.root.parentElement;
    const z = narrow() ? Math.min(1, frame.clientWidth / 400) : 0.75;
    w.root.style.setProperty("--z", String(z));
  }
  drawWires();
}

function setUserPaused(p) {
  userPaused = p;
  toggle.setAttribute("aria-pressed", String(p));
  toggle.textContent = p ? "Play demo" : "Pause demo";
  updatePause();
}

async function tickLoop() {
  while (true) {
    await new Promise((r) => setTimeout(r, 2000));
    if (isPaused() || fastMode) continue;
    const active = new Set();
    for (const v of Object.values(views)) for (const x of v.sessions) if (!x.idle) active.add(x.id);
    for (const id of active) {
      const t = ticks.get(id);
      if (!t) continue;
      t.minutes += 1;
      t.tokens += 80 + Math.floor(Math.random() * 321);
    }
    wins.mara.renderSessions(views.mara, false);
    wins.teo.renderSessions(views.teo, false);
  }
}

function start() {
  layout();
  new ResizeObserver(layout).observe(grid);
  stepper.addEventListener("click", (e) => {
    const b = e.target.closest("button[data-step]");
    if (b) jump(Number(b.dataset.step));
  });
  toggle.addEventListener("click", () => setUserPaused(!userPaused));
  new IntersectionObserver(
    ([entry]) => { autoPaused = entry.intersectionRatio < 0.2; updatePause(); },
    { threshold: [0, 0.2, 0.5] },
  ).observe(stage);
  document.addEventListener("visibilitychange", updatePause);
  if (fastMode) toggle.parentElement.querySelector("span").textContent = "A made-up room. Press the buttons in your window.";
  toggle.hidden = fastMode;
  reducedMotion.addEventListener("change", () => location.reload());
  for (const w of Object.values(wins)) startDrops(w.list, ".task.is-building", () => !isPaused());
  for (const w of Object.values(wins)) startDrops(w.sessionsEl, ".session.is-active", () => !isPaused());
  tickLoop();
  jump(1);
}

start();
