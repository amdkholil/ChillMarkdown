<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import MarkdownIt from "markdown-it";
import hljs from "highlight.js";
import * as katexPluginModule from "@vscode/markdown-it-katex";
import "katex/dist/katex.min.css";
import { remark } from "remark";
import { visit } from "unist-util-visit";

const ZOOM_MIN = 0.5;
const ZOOM_MAX = 3;
const ZOOM_STEP = 0.1;

/* ---------- markdown-it setup ---------- */

function slugify(text) {
  return (
    text
      .trim()
      .toLowerCase()
      .replace(/[^\p{L}\p{N}_\s-]/gu, "")
      .replace(/\s+/g, "-") || "section"
  );
}

function escapeHtmlAttr(str) {
  return encodeURIComponent(str);
}

const md = new MarkdownIt({
  html: true,
  linkify: true,
  typographer: true,
  highlight(code, lang) {
    if (lang === "mermaid") {
      return `<div class="mermaid-block" data-code="${escapeHtmlAttr(code)}"><div class="mermaid-hint">Mermaid rendering…</div></div>`;
    }
    if (lang && hljs.getLanguage(lang)) {
      return `<pre class="hljs"><code>${hljs.highlight(code, { language: lang, ignoreIllegals: true }).value}</code></pre>`;
    }
    return `<pre class="hljs"><code>${md.utils.escapeHtml(code)}</code></pre>`;
  },
});

const katexPlugin = katexPluginModule.default?.default ?? katexPluginModule.default ?? katexPluginModule;
md.use(katexPlugin);

md.core.ruler.push("chill_heading_ids", (state) => {
  const used = new Map();
  const tokens = state.tokens;
  for (let i = 0; i < tokens.length; i++) {
    const tok = tokens[i];
    if (tok.type !== "heading_open") continue;
    const inline = tokens[i + 1];
    const text =
      inline?.children
        ?.filter((c) => c.type === "text" || c.type === "code_inline")
        .map((c) => c.content)
        .join(" ") ?? "";
    const base = slugify(text);
    let id = base;
    let n = 1;
    while (used.has(id)) id = `${base}-${n++}`;
    used.set(id, true);
    tok.attrSet("id", id);
  }
});

/* ---------- state ---------- */

const DEFAULT_MD = `# Chill Markdown

Welcome. This is a markdown **viewer** with **LaTeX** ($e^{i\\pi}+1=0$),
**mermaid diagrams**, in-document navigation in the right panel, and zoom
(hold **Ctrl/Cmd + scroll**).

Click the **Edit** button (pencil icon) to open the split editor on the left.

## Features

- Light & dark mode (sun/moon icon in the toolbar)
- LaTeX block math:
  $$\\int_0^\\infty e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}$$
- Mermaid diagrams:

\`\`\`mermaid
flowchart LR
  A[Markdown] --> B{Mermaid?}
  B -->|Yes| C[Render diagram]
  B -->|No| D[Render text]
\`\`\`

\`\`\`mermaid
sequenceDiagram
  participant U as User
  participant W as Webview
  participant R as Rust
  U->>W: Click "Export PDF"
  W->>R: save dialog (defaults to Documents)
  R-->>W: path
  W->>R: save_file_bytes
  W-->>U: PDF saved
\`\`\`

## Code

\`\`\`rust
fn main() {
    println!("Hello, Chill Markdown!");
}
\`\`\`

## Table

| Feature | Status |
|---|---|
| Mermaid | ✅ |
| LaTeX | ✅ |
| Zoom | ✅ |
`;

/* ---------- tabs state ---------- */

let tabSeq = 0;
let untitledSeq = 0;

function makeTab(path = null, text = "") {
  const label = path ? null : (untitledSeq += 1) === 1 ? "untitled.md" : `untitled-${untitledSeq}.md`;
  return {
    id: ++tabSeq,
    path,
    label,
    content: text,
    dirty: false,
    history: [text],
    historyIndex: 0,
    historyLock: false,
  };
}

const tabs = ref([]);
const activeTabId = ref(null);

const activeTab = computed(
  () => tabs.value.find((t) => t.id === activeTabId.value) ?? null
);

const content = computed({
  get: () => activeTab.value?.content ?? "",
  set: (v) => {
    if (!activeTab.value) return;
    activeTab.value.content = v;
    activeTab.value.dirty = true;
  },
});

const currentPath = computed({
  get: () => activeTab.value?.path ?? null,
  set: (v) => {
    if (!activeTab.value) return;
    activeTab.value.path = v;
    activeTab.value.label = null;
  },
});

const dirty = computed({
  get: () => activeTab.value?.dirty ?? false,
  set: (v) => {
    if (!activeTab.value) return;
    activeTab.value.dirty = v;
  },
});

const theme = ref(localStorage.getItem("cm:theme") ?? (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"));
const zoom = ref(Number(localStorage.getItem("cm:zoom")) || 1);
const editing = ref(false);
const exporting = ref(false);
const exportProgress = ref("");
const exportPercent = ref(0);
const exportResultModal = ref({
  show: false,
  path: "",
  fileName: "",
});
const previewTarget = ref("");
const activeId = ref(null);
const toast = ref(null);
const previewRef = ref(null);
const editorRef = ref(null);

const toc = ref([]);

function loadSaved() {
  try {
    return localStorage.getItem("cm:content") ?? DEFAULT_MD;
  } catch {
    return DEFAULT_MD;
  }
}

/* ---------- undo / redo & editor history ---------- */

const canUndo = computed(() => {
  const tab = activeTab.value;
  return tab && (tab.historyIndex ?? 0) > 0;
});

const canRedo = computed(() => {
  const tab = activeTab.value;
  return tab && (tab.historyIndex ?? 0) < ((tab.history?.length ?? 1) - 1);
});

function recordHistory(tab, val) {
  if (!tab) return;
  if (!tab.history) {
    tab.history = [val];
    tab.historyIndex = 0;
    return;
  }
  if (tab.historyLock) return;
  if (tab.history[tab.historyIndex] === val) return;

  // truncate redo stack
  tab.history = tab.history.slice(0, tab.historyIndex + 1);
  tab.history.push(val);
  if (tab.history.length > 100) {
    tab.history.shift();
  }
  tab.historyIndex = tab.history.length - 1;
}

function undo() {
  const tab = activeTab.value;
  if (!tab || (tab.historyIndex ?? 0) <= 0) return;
  tab.historyLock = true;
  tab.historyIndex -= 1;
  const prevVal = tab.history[tab.historyIndex];
  tab.content = prevVal;
  tab.dirty = true;
  nextTick(() => {
    tab.historyLock = false;
  });
}

function redo() {
  const tab = activeTab.value;
  if (!tab || !tab.history || tab.historyIndex >= tab.history.length - 1) return;
  tab.historyLock = true;
  tab.historyIndex += 1;
  const nextVal = tab.history[tab.historyIndex];
  tab.content = nextVal;
  tab.dirty = true;
  nextTick(() => {
    tab.historyLock = false;
  });
}

let debounceTimer = null;
watch(content, (val) => {
  const tab = activeTab.value;
  recordHistory(tab, val);
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    previewTarget.value = val;
    try {
      localStorage.setItem("cm:content", val);
    } catch {}
  }, 250);
});

watch(theme, async (val) => {
  localStorage.setItem("cm:theme", val);
  document.documentElement.dataset.theme = val;
  mermaidThemeInitialized = null;
  await nextTick();
  await renderMermaidBlocks();
});

watch(zoom, (val) => {
  localStorage.setItem("cm:zoom", String(val));
});

const previewHtml = computed(() => md.render(previewTarget.value));

function tabLabel(tab) {
  if (!tab) return "";
  if (tab.path) return tab.path.split(/[/\\]/).pop();
  return tab.label ?? "untitled.md";
}

const fileName = computed(() => activeTab.value ? tabLabel(activeTab.value) : "");

const zoomPercent = computed(() => `${Math.round(zoom.value * 100)}%`);
const canZoomIn = computed(() => zoom.value < ZOOM_MAX - 0.001);
const canZoomOut = computed(() => zoom.value > ZOOM_MIN + 0.001);

/* ---------- mermaid ---------- */

let mermaidCache = null;
let mermaidThemeInitialized = null;

async function getMermaid() {
  if (!mermaidCache) {
    mermaidCache = await import("mermaid");
  }
  const lib = mermaidCache.default;
  const wantTheme = theme.value === "dark" ? "dark" : "default";
  if (mermaidThemeInitialized !== wantTheme) {
    lib.initialize({
      startOnLoad: false,
      theme: wantTheme,
      securityLevel: "loose",
      fontSize: 11,
      flowchart: { useMaxWidth: false },
      sequence: { useMaxWidth: false },
      gantt: { useMaxWidth: false },
      journey: { useMaxWidth: false },
      class: { useMaxWidth: false },
      state: { useMaxWidth: false },
      er: { useMaxWidth: false },
      pie: { useMaxWidth: false },
      gitGraph: {
        useMaxWidth: false,
        themeVariables: {
          git0: "#4c7dd1",
          gitBranchLabelFontSize: "11px",
          gitCommitLabelFontSize: "10px",
          gitTagLabelFontSize: "10px",
        },
      },
      themeVariables: {
        fontSize: "11px",
      },
    });
    mermaidThemeInitialized = wantTheme;
  }
  return lib;
}

let renderSeq = 0;
let mermaidSeq = 0;

/* ---------- mermaid auto-detection (remark AST) ---------- */

const MERMAID_KEYWORDS = [
  "graph",
  "flowchart",
  "sequenceDiagram",
  "classDiagram",
  "stateDiagram",
  "stateDiagram-v2",
  "erDiagram",
  "journey",
  "gantt",
  "pie",
  "quadrantChart",
  "requirementDiagram",
  "gitGraph",
  "mindmap",
  "timeline",
  "sankey-beta",
  "xychart-beta",
  "block-beta",
  "packet-beta",
  "architecture-beta",
  "kanban",
  "C4Context",
  "C4Container",
  "C4Component",
  "C4Dynamic",
  "C4Deployment",
];

function isMermaidKeyword(line) {
  const trimmed = line.trim();
  for (const kw of MERMAID_KEYWORDS) {
    if (trimmed === kw || trimmed.startsWith(kw + " ") || trimmed.startsWith(kw + "\n") || trimmed.startsWith(kw + ":")) {
      return true;
    }
  }
  return false;
}

function findFirstMeaningfulLine(code) {
  const lines = code.split("\n");
  let inFrontmatter = false;
  let inDirective = false;

  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const trimmed = raw.trim();

    if (!trimmed) continue;

    // Handle frontmatter --- ... ---
    if (!inFrontmatter && trimmed === "---" && i === 0) {
      inFrontmatter = true;
      continue;
    }
    if (inFrontmatter) {
      if (trimmed === "---") {
        inFrontmatter = false;
      }
      continue;
    }

    // Handle directives %%{init: ...}%% (single-line or multi-line)
    if (trimmed.startsWith("%%{")) {
      if (!trimmed.endsWith("}%%")) {
        inDirective = true;
      }
      continue;
    }
    if (inDirective) {
      if (trimmed.endsWith("}%%")) {
        inDirective = false;
      }
      continue;
    }

    // Handle comment %% ...
    if (trimmed.startsWith("%%")) {
      continue;
    }

    return trimmed;
  }
  return null;
}

async function detectCandidateMermaidBlocks(markdownText) {
  if (!markdownText) return [];
  const candidates = [];
  try {
    const ast = remark.parse(markdownText);
    visit(ast, "code", (node) => {
      const lang = (node.lang || "").trim().toLowerCase();
      if (!lang || lang === "text") {
        const val = node.value || "";
        const firstLine = findFirstMeaningfulLine(val);
        if (firstLine && isMermaidKeyword(firstLine)) {
          candidates.push(val);
        }
      }
    });
  } catch {}
  return candidates;
}

function postProcessMermaidSvg(blockEl) {
  const svg = blockEl.querySelector("svg");
  if (!svg) return;
  svg.style.maxWidth = "100%";
  svg.style.height = "auto";
  const vb = svg.getAttribute("viewBox");
  if (vb) {
    const parts = vb.split(/[\s,]+/).map(Number);
    if (parts.length === 4 && parts[2] > 0) {
      const vbWidth = parts[2];
      // Jika pie chart atau diagram pie
      if (svg.classList.contains("pieCircle") || svg.querySelector(".pieCircle, .legend, .pieTitleText")) {
        const scaledWidth = Math.min(vbWidth * 0.7, 450);
        svg.style.width = `${Math.round(scaledWidth)}px`;
      } else if (svg.classList.contains("gitgraph") || svg.querySelector(".commit, .branch")) {
        const scaledWidth = Math.min(vbWidth * 0.75, 550);
        svg.style.width = `${Math.round(scaledWidth)}px`;
      } else {
        svg.style.width = `${Math.round(vbWidth)}px`;
      }
    }
  }
}

async function renderMermaidBlocks() {
  const root = previewRef.value;
  if (!root) return;
  const currentSeq = ++renderSeq;
  const mermaid = await getMermaid();
  if (currentSeq !== renderSeq) return;

  // 1. Render explicit ```mermaid blocks
  const explicitBlocks = root.querySelectorAll(".mermaid-block[data-code]");
  for (const block of explicitBlocks) {
    if (currentSeq !== renderSeq) break;
    const code = decodeURIComponent(block.dataset.code);
    try {
      const id = `mmd-${Date.now()}-${++mermaidSeq}`;
      const { svg } = await mermaid.render(id, code);
      if (currentSeq !== renderSeq) break;
      block.innerHTML = svg;
      postProcessMermaidSvg(block);
    } catch (err) {
      if (currentSeq !== renderSeq) break;
      block.innerHTML = `<div class="mermaid-error">Mermaid error: ${md.utils.escapeHtml(String(err?.message ?? err))}</div>`;
    }
  }

  // 2. Detect & validate unlabelled / text-labelled mermaid blocks via remark AST
  const candidates = await detectCandidateMermaidBlocks(previewTarget.value);
  if (!candidates.length || currentSeq !== renderSeq) return;

  // Find all pre > code in preview that match candidate values
  const preCodes = root.querySelectorAll("pre > code");
  for (const codeEl of preCodes) {
    if (currentSeq !== renderSeq) break;
    const pre = codeEl.parentElement;
    if (!pre || pre.classList.contains("mermaid-converted")) continue;

    const rawCode = codeEl.textContent.replace(/\r\n/g, "\n").trim();
    const candidate = candidates.find((c) => c.replace(/\r\n/g, "\n").trim() === rawCode);
    if (!candidate) continue;

    // Validate candidate with mermaid.parse(code, { suppressErrors: true })
    try {
      const isValid = await mermaid.parse(candidate, { suppressErrors: true });
      if (!isValid || currentSeq !== renderSeq) continue;

      const id = `mmd-${Date.now()}-${++mermaidSeq}`;
      const { svg } = await mermaid.render(id, candidate);
      if (currentSeq !== renderSeq) break;

      const holder = document.createElement("div");
      holder.className = "mermaid-block mermaid-converted";
      holder.innerHTML = svg;
      pre.replaceWith(holder);
      postProcessMermaidSvg(holder);
    } catch {}
  }
}

/* ---------- render pipeline ---------- */

function buildToc(src) {
  try {
    const tokens = md.parse(src, {});
    const out = [];
    let head = null;
    for (const tok of tokens) {
      if (tok.type === "heading_open") {
        head = { level: Number(tok.tag.slice(1)), id: tok.attrGet("id") };
      } else if (tok.type === "inline" && head) {
        const text =
          tok.children
            ?.filter((c) => c.type === "text" || c.type === "code_inline")
            .map((c) => c.content)
            .join(" ") ?? "";
        head.text = text;
        out.push(head);
        head = null;
      }
    }
    return out;
  } catch {
    return [];
  }
}

let observer = null;

function attachScrollspy() {
  observer?.disconnect();
  observer = null;
  activeId.value = null;
  const root = previewRef.value;
  if (!root) return;
  const headings = root.querySelectorAll("h1[id], h2[id], h3[id], h4[id], h5[id], h6[id]");
  if (!headings.length) return;
  observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          activeId.value = entry.target.id;
        }
      }
    },
    { rootMargin: "0px 0px -70% 0px", threshold: 0 }
  );
  headings.forEach((h) => observer.observe(h));
  activeId.value = headings[0].id;
}

watch(previewHtml, async (html) => {
  toc.value = buildToc(previewTarget.value);
  await nextTick();
  await renderMermaidBlocks();
  attachScrollspy();
});

/* ---------- zoom & wheel ---------- */

function clampZoom(z) {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(z * 100) / 100));
}

function zoomIn() {
  zoom.value = clampZoom(zoom.value + ZOOM_STEP);
}

function zoomOut() {
  zoom.value = clampZoom(zoom.value - ZOOM_STEP);
}

function zoomReset() {
  zoom.value = 1;
}

function onWheel(e) {
  if (!(e.ctrlKey || e.metaKey)) return;
  e.preventDefault();
  const dir = e.deltaY > 0 ? -1 : 1;
  zoom.value = clampZoom(zoom.value + dir * ZOOM_STEP);
}

const hasPrimaryModifier = (e) => e.ctrlKey || e.metaKey;

function onKeydown(e) {
  if (e.key === "Escape") {
    if (exportResultModal.value.show) {
      exportResultModal.value.show = false;
    } else if (tabContextMenu.value.show) {
      closeTabContextMenu();
    } else if (showSearch.value) {
      closeSearch();
    } else if (showAbout.value) {
      showAbout.value = false;
    } else if (showProps.value) {
      showProps.value = false;
    } else if (editing.value) {
      editing.value = false;
    }
    return;
  }
  if (hasPrimaryModifier(e) && e.key.toLowerCase() === "h") {
    e.preventDefault();
    openSearch(true);
  } else if (hasPrimaryModifier(e) && e.key.toLowerCase() === "f") {
    e.preventDefault();
    openSearch();
  } else if (hasPrimaryModifier(e) && e.key.toLowerCase() === "b") {
    e.preventDefault();
    toggleSidebar();
  } else if (hasPrimaryModifier(e) && e.key.toLowerCase() === "z" && !e.shiftKey) {
    if (editing.value && !e.target.matches("input")) {
      e.preventDefault();
      undo();
    }
  } else if (
    (hasPrimaryModifier(e) && e.key.toLowerCase() === "y") ||
    (hasPrimaryModifier(e) && e.shiftKey && e.key.toLowerCase() === "z")
  ) {
    if (editing.value && !e.target.matches("input")) {
      e.preventDefault();
      redo();
    }
  } else if (hasPrimaryModifier(e) && (e.key === "=" || e.key === "+")) {
    e.preventDefault();
    zoomIn();
  } else if (hasPrimaryModifier(e) && e.key === "-") {
    e.preventDefault();
    zoomOut();
  } else if (hasPrimaryModifier(e) && e.key === "0") {
    e.preventDefault();
    zoomReset();
  } else if (hasPrimaryModifier(e) && e.key.toLowerCase() === "s") {
    e.preventDefault();
    saveFile();
  } else if (hasPrimaryModifier(e) && e.key.toLowerCase() === "u") {
    e.preventDefault();
    checkForUpdates(false);
  } else if (hasPrimaryModifier(e) && e.key.toLowerCase() === "t") {
    e.preventDefault();
    newTab();
  } else if (hasPrimaryModifier(e) && e.key.toLowerCase() === "w") {
    e.preventDefault();
    if (activeTabId.value) {
      closeTab(activeTabId.value);
    }
  } else if (hasPrimaryModifier(e) && e.altKey && e.key === "ArrowRight") {
    e.preventDefault();
    cycleTab(1);
  } else if (hasPrimaryModifier(e) && e.altKey && e.key === "ArrowLeft") {
    e.preventDefault();
    cycleTab(-1);
  }
}

function cycleTab(dir) {
  const idx = tabs.value.findIndex((t) => t.id === activeTabId.value);
  if (idx < 0 || tabs.value.length < 2) return;
  const next = (idx + dir + tabs.value.length) % tabs.value.length;
  activateTab(tabs.value[next].id);
}

function onTabAuxClick(id, e) {
  if (e.button === 1) closeTab(id);
}

function onEditorKeydown(e) {
  if (e.key !== "Tab") return;
  e.preventDefault();
  const el = e.currentTarget;
  const start = el.selectionStart;
  const end = el.selectionEnd;
  content.value = content.value.slice(0, start) + "  " + content.value.slice(end);
  nextTick(() => {
    el.selectionStart = el.selectionEnd = start + 2;
  });
}

/* ---------- preview interactions ---------- */

function onPreviewClick(e) {
  const anchor = e.target.closest("a");
  if (!anchor) return;
  const href = anchor.getAttribute("href") ?? "";
  if (href.startsWith("#")) {
    e.preventDefault();
    const id = decodeURIComponent(href.slice(1));
    scrollToHeading(id);
  } else if (/^https?:/i.test(href)) {
    e.preventDefault();
    openUrl(href);
  }
}

function scrollToHeading(id) {
  const el = previewRef.value?.querySelector(`#${CSS.escape(id)}`);
  if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
}

/* ---------- toast ---------- */

let toastTimer = null;

function showToast(msg) {
  toast.value = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toast.value = null;
  }, 2500);
}

/* ---------- file ops ---------- */

async function documentsDir() {
  try {
    return await invoke("documents_dir");
  } catch {
    return null;
  }
}

/* ---------- tabs ops ---------- */

async function activateTab(id) {
  const tab = tabs.value.find((t) => t.id === id);
  if (!tab) return;
  activeTabId.value = id;
  previewTarget.value = tab.content;
  try {
    localStorage.setItem("cm:content", tab.content);
  } catch {}
  await nextTick();
  await renderMermaidBlocks();
  showToast("Switched to " + tabLabel(tab));
}

async function openFile() {
  const path = await invoke("show_open_dialog");
  if (!path) return;
  try {
    const text = await invoke("read_file", { path });
    const existing = tabs.value.find((t) => t.path === path);
    if (existing) {
      existing.content = text;
      existing.dirty = false;
      activateTab(existing.id);
    } else {
      const tab = makeTab(path, text);
      tabs.value.push(tab);
      activateTab(tab.id);
    }
    currentPath.value = path;
    dirty.value = false;
    try {
      localStorage.setItem("cm:content", text);
    } catch {}
    showToast("Opened: " + fileName.value);
  } catch (err) {
    showToast("Failed to open file: " + err);
  }
}

function newTab() {
  const tab = makeTab();
  tabs.value.push(tab);
  activateTab(tab.id);
  editing.value = true;
}

async function closeTab(id) {
  const idx = tabs.value.findIndex((t) => t.id === id);
  if (idx < 0) return;
  const tab = tabs.value[idx];
  if (tab.dirty) {
    const ok = await invoke("confirm_discard", {
      message: `Discard unsaved changes in “${tabLabel(tab)}”?`,
    });
    if (!ok) return;
  }
  tabs.value.splice(idx, 1);
  if (activeTabId.value === id) {
    const next = tabs.value[Math.max(0, idx - 1)];
    if (next) {
      await activateTab(next.id);
    } else {
      activeTabId.value = null;
      previewTarget.value = "";
      editing.value = false;
    }
  }
}

async function closeOtherTabs(id) {
  const target = tabs.value.find((t) => t.id === id);
  if (!target) return;
  const others = tabs.value.filter((t) => t.id !== id);
  const dirtyOthers = others.filter((t) => t.dirty);
  if (dirtyOthers.length > 0) {
    const ok = await invoke("confirm_discard", {
      message: `Discard unsaved changes in ${dirtyOthers.length} other tab(s)?`,
    });
    if (!ok) return;
  }
  tabs.value = [target];
  await activateTab(target.id);
}

async function closeTabsToRight(id) {
  const idx = tabs.value.findIndex((t) => t.id === id);
  if (idx < 0 || idx >= tabs.value.length - 1) return;
  const rights = tabs.value.slice(idx + 1);
  const dirtyRights = rights.filter((t) => t.dirty);
  if (dirtyRights.length > 0) {
    const ok = await invoke("confirm_discard", {
      message: `Discard unsaved changes in ${dirtyRights.length} tab(s) to the right?`,
    });
    if (!ok) return;
  }
  tabs.value = tabs.value.slice(0, idx + 1);
  if (!tabs.value.some((t) => t.id === activeTabId.value)) {
    await activateTab(id);
  }
}

async function closeTabsToLeft(id) {
  const idx = tabs.value.findIndex((t) => t.id === id);
  if (idx <= 0) return;
  const lefts = tabs.value.slice(0, idx);
  const dirtyLefts = lefts.filter((t) => t.dirty);
  if (dirtyLefts.length > 0) {
    const ok = await invoke("confirm_discard", {
      message: `Discard unsaved changes in ${dirtyLefts.length} tab(s) to the left?`,
    });
    if (!ok) return;
  }
  tabs.value = tabs.value.slice(idx);
  if (!tabs.value.some((t) => t.id === activeTabId.value)) {
    await activateTab(id);
  }
}

async function saveFile() {
  if (!activeTab.value) return;
  let path = currentPath.value;
  if (!path) {
    const dir = await documentsDir();
    path = await invoke("show_save_dialog", { defaultDir: dir, fileName: "untitled.md" });
    if (!path) return;
  }
  try {
    await invoke("save_file", { path, contents: content.value });
    currentPath.value = path;
    dirty.value = false;
    showToast("Saved: " + fileName.value);
  } catch (err) {
    showToast("Failed to save: " + err);
  }
}

async function saveFileAs() {
  if (!activeTab.value) return;
  const dir = await documentsDir();
  const path = await invoke("show_save_dialog", {
    defaultDir: dir,
    fileName: fileName.value || "untitled.md",
  });
  if (!path) return;
  try {
    await invoke("save_file", { path, contents: content.value });
    currentPath.value = path;
    dirty.value = false;
    showToast("Saved as: " + fileName.value);
  } catch (err) {
    showToast("Failed to save: " + err);
  }
}

/* ---------- export PDF ---------- */

async function exportPdf() {
  if (!activeTab.value || exporting.value) return;
  try {
    const baseName = fileName.value.replace(/\.(md|markdown|txt)$/i, "") || "document";
    const dir = await documentsDir();
    const path = await invoke("show_save_dialog", {
      defaultDir: dir,
      fileName: `${baseName}.pdf`,
    });
    if (!path) return;

    // Tampilkan modal progress seketika
    exporting.value = true;
    exportPercent.value = 15;
    exportProgress.value = "Menyiapkan elemen dokumen…";
    await nextTick();
    await new Promise((r) => setTimeout(r, 60));

    const holder = document.createElement("div");
    holder.className = "md-content print-flow";
    holder.dataset.theme = "light";
    holder.style.display = "block";
    holder.style.width = "100%";
    holder.style.maxWidth = "100%";
    holder.style.padding = "0";
    holder.style.boxSizing = "border-box";
    holder.style.backgroundColor = "#ffffff";
    holder.style.color = "#1c2024";

    // Tetapkan custom properties standar dokumen kertas (light)
    holder.style.setProperty("--bg", "#ffffff");
    holder.style.setProperty("--fg", "#1c2024");
    holder.style.setProperty("--border", "#d8dde5");
    holder.style.setProperty("--panel", "#ffffff");
    holder.style.setProperty("--panel-alt", "#eef0f3");
    holder.style.setProperty("--code-bg", "#f0f2f5");
    holder.style.setProperty("--accent", "#4c7dd1");

    holder.innerHTML = previewRef.value.innerHTML;

    // Pastikan SVG (seperti Mermaid) memiliki dimensi fisik yang terukur dan font 11px
    const originalSvgs = previewRef.value.querySelectorAll("svg");
    const clonedSvgs = holder.querySelectorAll("svg");
    clonedSvgs.forEach((svg, idx) => {
      const orig = originalSvgs[idx];
      if (orig) {
        const rect = orig.getBoundingClientRect();
        if (rect.width > 0) {
          svg.style.width = `${Math.round(rect.width)}px`;
          svg.setAttribute("width", String(Math.round(rect.width)));
        }
        if (rect.height > 0) {
          svg.style.height = `${Math.round(rect.height)}px`;
          svg.setAttribute("height", String(Math.round(rect.height)));
        }
      }
      if (svg.closest(".mermaid-block")) {
        svg.style.maxWidth = "100%";
        svg.style.fontSize = "11px";
        svg.querySelectorAll("text, .label text, .node text").forEach((t) => {
          t.style.fontSize = "11px";
        });
      }
    });

    document.body.appendChild(holder);
    exportPercent.value = 35;
    exportProgress.value = "Memproses tata letak halaman…";
    await new Promise((r) => setTimeout(r, 60));

    try {
      exportPercent.value = 55;
      exportProgress.value = "Merender halaman PDF…";
      const html2pdf = (await import("html2pdf.js/dist/html2pdf.js")).default;
      const opt = {
        margin: [15, 15, 15, 15],
        image: { type: "jpeg", quality: 0.98 },
        html2canvas: {
          scale: 2,
          useCORS: true,
          backgroundColor: "#ffffff",
        },
        jsPDF: { unit: "mm", format: "a4", orientation: "portrait" },
        pagebreak: { mode: ["css", "legacy"] },
      };
      const blob = await html2pdf().set(opt).from(holder).outputPdf("blob");
      exportPercent.value = 85;
      exportProgress.value = "Menyimpan file PDF…";
      const buf = new Uint8Array(await blob.arrayBuffer());
      await invoke("save_file_bytes", { path, bytes: Array.from(buf) });
      
      exportPercent.value = 100;
      exportProgress.value = "Selesai!";
      await new Promise((r) => setTimeout(r, 80));

      const savedName = path.split(/[/\\]/).pop();
      exportResultModal.value = {
        show: true,
        path,
        fileName: savedName,
      };
    } finally {
      holder.remove();
    }
  } catch (err) {
    showToast("Failed to export PDF: " + err);
  } finally {
    exporting.value = false;
    exportProgress.value = "";
    exportPercent.value = 0;
  }
}

async function openExportedPdf() {
  const p = exportResultModal.value.path;
  if (!p) return;
  try {
    if (isTauri()) {
      await invoke("open_system_path", { path: p });
    } else {
      await openPath(p);
    }
  } catch (err) {
    showToast("Failed to open file: " + err);
  }
}

async function showExportedInFolder() {
  const p = exportResultModal.value.path;
  if (!p) return;
  try {
    if (isTauri()) {
      await invoke("show_in_folder", { path: p });
    } else {
      await openPath(p);
    }
  } catch (err) {
    showToast("Failed to open folder: " + err);
  }
}

/* ---------- editing toggle ---------- */

watch(editing, async (val) => {
  if (val) {
    await nextTick();
    editorRef.value?.focus();
  } else {
    clearTimeout(debounceTimer);
    previewTarget.value = content.value;
    await nextTick();
    await renderMermaidBlocks();
  }
});

function toggleEdit() {
  if (!activeTab.value) return;
  editing.value = !editing.value;
}

/* ---------- file properties modal ---------- */

const showProps = ref(false);
const fileMeta = ref(null);

const fileStats = computed(() => {
  const text = content.value;
  const trimmed = text.trim();
  const words = trimmed ? trimmed.split(/\s+/).length : 0;
  const lines = text.split("\n").length;
  const fences = (text.match(/^```/gm) || []).length;
  const mermaidCount = (text.match(/^```mermaid$/gm) || []).length;
  return {
    words,
    chars: text.length,
    lines,
    headings: toc.value.length,
    blocks: Math.floor(fences / 2),
    mermaid: mermaidCount,
    readingMinutes: words ? Math.max(1, Math.ceil(words / 200)) : 0,
  };
});

function formatBytes(n) {
  if (n == null) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

function formatTime(millis) {
  if (millis == null) return "—";
  return new Date(millis).toLocaleString();
}

const propsRows = computed(() => {
  const meta = fileMeta.value ?? {};
  const stats = fileStats.value;
  return [
    ["Name", fileName.value],
    ["Path", currentPath.value ?? "— (draft, not saved to disk)"],
    ["Type", /\.(md|markdown)$/i.test(fileName.value) ? "Markdown" : "Plain text"],
    ["Size", formatBytes(meta.size)],
    ["Created", formatTime(meta.created_millis)],
    ["Modified", formatTime(meta.modified_millis)],
    ["Words", stats.words.toLocaleString()],
    ["Characters", stats.chars.toLocaleString()],
    ["Lines", stats.lines.toLocaleString()],
    ["Headings", String(stats.headings)],
    ["Fenced code blocks", String(stats.blocks)],
    ["Mermaid diagrams", String(stats.mermaid)],
    ["Est. reading time", stats.words ? `~${stats.readingMinutes} min` : "—"],
  ];
});

const propsError = ref(null);

/* ---------- about modal ---------- */

const showAbout = ref(false);
const appVersion = ref("—");

async function openAbout() {
  showAbout.value = true;
  try {
    const { getVersion } = await import("@tauri-apps/api/app");
    appVersion.value = (await getVersion()) || "—";
  } catch {
    appVersion.value = "—";
  }
}

async function openGithub() {
  try {
    await openUrl("https://github.com/amdkholil/ChillMarkdown");
  } catch {}
}

async function openIssues() {
  try {
    await openUrl("https://github.com/amdkholil/ChillMarkdown/issues");
  } catch {}
}

async function openBmc() {
  try {
    await openUrl("https://buymeacoffee.com/amdkholil");
  } catch {}
}

async function openProperties() {
  if (!activeTab.value) return;
  fileMeta.value = null;
  propsError.value = null;
  showProps.value = true;
  if (currentPath.value && isTauri()) {
    try {
      fileMeta.value = await invoke("file_info", { path: currentPath.value });
    } catch (err) {
      propsError.value = String(err);
    }
  }
}

/* ---------- auto updater ---------- */

const updateInfo = ref(null);
const rawUpdate = shallowRef(null);
const updating = ref(false);
const checkingUpdate = ref(false);

async function checkForUpdates(silent = false) {
  if (!isTauri()) return;
  checkingUpdate.value = true;
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (update) {
      rawUpdate.value = update;
      updateInfo.value = {
        version: update.version,
        notes: update.body,
      };
      showToast("Update v" + update.version + " available — click the update button in the toolbar");
    } else if (!silent) {
      showToast("You're on the latest version");
    }
  } catch (err) {
    if (!silent) showToast("Failed to check for updates: " + err);
  } finally {
    checkingUpdate.value = false;
  }
}

async function installUpdate() {
  const update = rawUpdate.value;
  if (!update || updating.value || dirty.value) {
    if (dirty.value) showToast("Save your changes before updating");
    return;
  }
  updating.value = true;
  try {
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await update.downloadAndInstall();
    showToast("Update installed — restarting…");
    await relaunch();
  } catch (err) {
    showToast("Failed to install update: " + err);
    updating.value = false;
  }
}

/* ---------- split editor resize ---------- */

const SPLIT_MIN = 0.15;
const SPLIT_MAX = 0.7;
const SPLIT_DEFAULT = 0.38;

const splitRatio = ref(loadSplitRatio());
const splitAreaRef = ref(null);
const isDraggingSplit = ref(false);

function loadSplitRatio() {
  const saved = Number(localStorage.getItem("cm:split"));
  return saved >= SPLIT_MIN && saved <= SPLIT_MAX ? saved : SPLIT_DEFAULT;
}

function setSplitRatio(r) {
  splitRatio.value = Math.min(
    SPLIT_MAX,
    Math.max(SPLIT_MIN, Math.round(r * 1000) / 1000)
  );
}

watch(splitRatio, (val) => {
  localStorage.setItem("cm:split", String(val));
});

const DIVIDER_OFFSET = "4px";

const editorStyle = computed(() => ({
  flex: "none",
  width: `calc(${(splitRatio.value * 100).toFixed(3)}% - ${DIVIDER_OFFSET})`,
}));

function onSplitPointerDown(e) {
  e.preventDefault();
  isDraggingSplit.value = true;
  e.currentTarget.setPointerCapture(e.pointerId);
}

function onSplitPointerMove(e) {
  if (!isDraggingSplit.value) return;
  const area = splitAreaRef.value;
  if (!area) return;
  const rect = area.getBoundingClientRect();
  if (rect.width <= 0) return;
  setSplitRatio((e.clientX - rect.left) / rect.width);
}

function onSplitPointerUp(e) {
  if (!isDraggingSplit.value) return;
  isDraggingSplit.value = false;
  try {
    e.currentTarget.releasePointerCapture(e.pointerId);
  } catch {}
}

function resetSplit() {
  splitRatio.value = SPLIT_DEFAULT;
}

function onSplitKeydown(e) {
  if (e.key === "ArrowLeft") {
    e.preventDefault();
    setSplitRatio(splitRatio.value - 0.02);
  } else if (e.key === "ArrowRight") {
    e.preventDefault();
    setSplitRatio(splitRatio.value + 0.02);
  }
}

/* ---------- sidebar / toc panel toggle & resize ---------- */

const TOC_MIN_WIDTH = 160;
const TOC_MAX_WIDTH = 500;
const TOC_DEFAULT_WIDTH = 250;

const showSidebar = ref(localStorage.getItem("cm:show-sidebar") !== "false");
const sidebarWidth = ref(loadSidebarWidth());
const isDraggingSidebar = ref(false);

function loadSidebarWidth() {
  const saved = Number(localStorage.getItem("cm:sidebar-width"));
  return saved >= TOC_MIN_WIDTH && saved <= TOC_MAX_WIDTH ? saved : TOC_DEFAULT_WIDTH;
}

function setSidebarWidth(w) {
  sidebarWidth.value = Math.min(
    TOC_MAX_WIDTH,
    Math.max(TOC_MIN_WIDTH, Math.round(w))
  );
}

watch(showSidebar, (val) => {
  localStorage.setItem("cm:show-sidebar", String(val));
});

watch(sidebarWidth, (val) => {
  localStorage.setItem("cm:sidebar-width", String(val));
});

function toggleSidebar() {
  showSidebar.value = !showSidebar.value;
}

function onSidebarPointerDown(e) {
  e.preventDefault();
  isDraggingSidebar.value = true;
  e.currentTarget.setPointerCapture(e.pointerId);
}

function onSidebarPointerMove(e) {
  if (!isDraggingSidebar.value) return;
  const newW = window.innerWidth - e.clientX;
  setSidebarWidth(newW);
}

function onSidebarPointerUp(e) {
  if (!isDraggingSidebar.value) return;
  isDraggingSidebar.value = false;
  try {
    e.currentTarget.releasePointerCapture(e.pointerId);
  } catch {}
}

function resetSidebarWidth() {
  sidebarWidth.value = TOC_DEFAULT_WIDTH;
}

function onSidebarKeydown(e) {
  if (e.key === "ArrowLeft") {
    e.preventDefault();
    setSidebarWidth(sidebarWidth.value + 15);
  } else if (e.key === "ArrowRight") {
    e.preventDefault();
    setSidebarWidth(sidebarWidth.value - 15);
  }
}

/* ---------- tab context menu ---------- */

const tabContextMenu = ref({
  show: false,
  x: 0,
  y: 0,
  tabId: null,
});

function onTabContextMenu(tabId, e) {
  e.preventDefault();
  e.stopPropagation();
  tabContextMenu.value = {
    show: true,
    x: e.clientX,
    y: e.clientY,
    tabId,
  };
}

function closeTabContextMenu() {
  tabContextMenu.value.show = false;
}

/* ---------- search & replace ---------- */

const showSearch = ref(false);
const searchQuery = ref("");
const replaceQuery = ref("");
const showReplace = ref(false);
const searchCaseSensitive = ref(false);
const searchIndex = ref(0);
const searchTotal = ref(0);
const searchInputRef = ref(null);
const replaceInputRef = ref(null);
const editorMatches = ref([]); // list of start indices in editor

function openSearch(withReplace = false) {
  showSearch.value = true;
  if (withReplace && editing.value) {
    showReplace.value = true;
  }
  nextTick(() => {
    searchInputRef.value?.focus();
    searchInputRef.value?.select();
    executeSearch(false);
  });
}

function closeSearch() {
  showSearch.value = false;
  showReplace.value = false;
  clearHighlights();
  if (editing.value) {
    editorRef.value?.focus();
  }
}

function toggleSearch() {
  if (showSearch.value) {
    closeSearch();
  } else {
    openSearch();
  }
}

function toggleReplace() {
  if (!editing.value) return;
  showReplace.value = !showReplace.value;
  if (showReplace.value) {
    nextTick(() => replaceInputRef.value?.focus());
  }
}

function clearHighlights() {
  searchIndex.value = 0;
  searchTotal.value = 0;
  editorMatches.value = [];
  const root = previewRef.value;
  if (!root) return;
  const marks = root.querySelectorAll("mark.cm-search-match");
  marks.forEach((m) => {
    const parent = m.parentNode;
    if (parent) {
      parent.replaceChild(document.createTextNode(m.textContent), m);
      parent.normalize();
    }
  });
  // Also clean up any lingering text nodes normalization in root
  root.normalize();
}

function executeSearch(focusMatch = false) {
  clearHighlights();
  const q = searchQuery.value;
  if (!q) return;

  if (editing.value && editorRef.value) {
    // Search in textarea
    const text = content.value;
    const flags = searchCaseSensitive.value ? "g" : "gi";
    const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const regex = new RegExp(escaped, flags);
    const matches = [];
    let match;
    while ((match = regex.exec(text)) !== null) {
      matches.push(match.index);
    }
    editorMatches.value = matches;
    searchTotal.value = matches.length;
    if (matches.length > 0) {
      searchIndex.value = 1;
      highlightEditorMatch(0, focusMatch);
    }
  } else if (previewRef.value) {
    // Search in preview DOM
    const root = previewRef.value;
    const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const regex = new RegExp(escaped, searchCaseSensitive.value ? "g" : "gi");

    // Walk text nodes
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        if (!node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
        const parentTag = node.parentElement?.tagName;
        if (
          parentTag === "SCRIPT" ||
          parentTag === "STYLE" ||
          parentTag === "MARK" ||
          parentTag === "NOSCRIPT"
        ) {
          return NodeFilter.FILTER_REJECT;
        }
        // Exclude SVG elements (like Mermaid rendered SVGs containing internal text/labels/classes)
        if (node.parentElement?.closest("svg, .katex-mathml")) {
          return NodeFilter.FILTER_REJECT;
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    });

    const nodes = [];
    while (walker.nextNode()) {
      nodes.push(walker.currentNode);
    }

    let count = 0;
    nodes.forEach((node) => {
      const text = node.nodeValue;
      if (!regex.test(text)) return;
      regex.lastIndex = 0;

      const frag = document.createDocumentFragment();
      let lastIdx = 0;
      let m;
      while ((m = regex.exec(text)) !== null) {
        if (m.index > lastIdx) {
          frag.appendChild(document.createTextNode(text.slice(lastIdx, m.index)));
        }
        const mark = document.createElement("mark");
        mark.className = "cm-search-match";
        mark.textContent = m[0];
        frag.appendChild(mark);
        lastIdx = regex.lastIndex;
        count++;
      }
      if (lastIdx < text.length) {
        frag.appendChild(document.createTextNode(text.slice(lastIdx)));
      }
      node.parentNode.replaceChild(frag, node);
    });

    searchTotal.value = count;
    if (count > 0) {
      searchIndex.value = 1;
      highlightPreviewMatch(0);
    }
  }
}

function highlightEditorMatch(idx, shouldFocus = true) {
  const matches = editorMatches.value;
  if (!matches.length || !editorRef.value) return;
  const pos = matches[idx];
  const qLen = searchQuery.value.length;
  const textarea = editorRef.value;
  if (shouldFocus) {
    textarea.focus();
  }
  textarea.setSelectionRange(pos, pos + qLen);

  // Scroll textarea into view
  const linesBefore = textarea.value.slice(0, pos).split("\n").length;
  const totalLines = textarea.value.split("\n").length;
  const scrollRatio = linesBefore / Math.max(1, totalLines);
  textarea.scrollTop = scrollRatio * textarea.scrollHeight - textarea.clientHeight / 2;
}

function highlightPreviewMatch(idx) {
  const root = previewRef.value;
  if (!root) return;
  const marks = root.querySelectorAll("mark.cm-search-match");
  marks.forEach((m, i) => {
    if (i === idx) {
      m.classList.add("cm-current");
      m.scrollIntoView({ behavior: "smooth", block: "center" });
    } else {
      m.classList.remove("cm-current");
    }
  });
}

function findNext() {
  if (searchTotal.value === 0) return;
  let next = searchIndex.value + 1;
  if (next > searchTotal.value) next = 1;
  searchIndex.value = next;
  if (editing.value) {
    highlightEditorMatch(next - 1);
  } else {
    highlightPreviewMatch(next - 1);
  }
}

function findPrev() {
  if (searchTotal.value === 0) return;
  let prev = searchIndex.value - 1;
  if (prev < 1) prev = searchTotal.value;
  searchIndex.value = prev;
  if (editing.value) {
    highlightEditorMatch(prev - 1);
  } else {
    highlightPreviewMatch(prev - 1);
  }
}

function replaceCurrent() {
  if (!editing.value || searchTotal.value === 0 || !editorMatches.value.length) return;
  const currentIdx = Math.max(0, searchIndex.value - 1);
  const pos = editorMatches.value[currentIdx];
  const qLen = searchQuery.value.length;
  const text = content.value;
  const rep = replaceQuery.value;
  content.value = text.slice(0, pos) + rep + text.slice(pos + qLen);
  nextTick(() => {
    executeSearch();
  });
}

function replaceAll() {
  if (!editing.value || searchTotal.value === 0) return;
  const q = searchQuery.value;
  if (!q) return;
  const flags = searchCaseSensitive.value ? "g" : "gi";
  const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const regex = new RegExp(escaped, flags);
  content.value = content.value.replace(regex, replaceQuery.value);
  nextTick(() => {
    executeSearch();
    showToast("Replaced matches");
  });
}

/* ---------- custom tooltip directive ---------- */

let tipEl = null;
let tipTimer = null;
let tipHost = null;

function ensureTipEl() {
  if (!tipEl) {
    tipEl = document.createElement("div");
    tipEl.className = "cm-tooltip";
    tipEl.setAttribute("role", "tooltip");
    document.body.appendChild(tipEl);
  }
  return tipEl;
}

function hideTip() {
  clearTimeout(tipTimer);
  tipTimer = null;
  tipHost = null;
  tipEl?.classList.remove("visible");
}

function showTip(host, text) {
  if (!text) return;
  const tip = ensureTipEl();
  tip.textContent = text;
  tip.classList.add("visible");
  tipHost = host;
  const rect = host.getBoundingClientRect();
  const tw = tip.offsetWidth;
  const th = tip.offsetHeight;
  const above = rect.top > th + 12;
  const left = Math.min(Math.max(8, rect.left + rect.width / 2 - tw / 2), window.innerWidth - tw - 8);
  const top = above ? rect.top - th - 8 : rect.top + rect.height + 8;
  tip.style.left = `${Math.round(left)}px`;
  tip.style.top = `${Math.round(top)}px`;
}

const vTooltip = {
  mounted(el, { value }) {
    let getText = () => value;
    const schedule = () => {
      if (el.disabled) return;
      clearTimeout(tipTimer);
      tipTimer = setTimeout(() => showTip(el, getText()), 300);
    };
    const showNow = () => {
      clearTimeout(tipTimer);
      tipTimer = null;
      showTip(el, getText());
    };
    const onDown = () => hideTip();
    el.addEventListener("mouseenter", schedule);
    el.addEventListener("mouseleave", hideTip);
    el.addEventListener("mousedown", onDown);
    el.addEventListener("focusin", showNow);
    el.addEventListener("focusout", hideTip);
    el.__vTooltip = { schedule, showNow, onDown, set text(v) { getText = () => v; }, get text() { return getText(); } };
    el.__vTooltip.scheduleHide = hideTip;
  },
  updated(el, { value }) {
    const t = el.__vTooltip;
    if (!t) return;
    t.text = value;
    if (tipHost === el && tipTimer === null) showTip(el, value);
  },
  unmounted(el) {
    const t = el.__vTooltip;
    if (t) {
      el.removeEventListener("mouseenter", t.schedule);
      el.removeEventListener("focusin", t.showNow);
      el.removeEventListener("mousedown", t.onDown);
      el.removeEventListener("mouseleave", t.scheduleHide);
      el.removeEventListener("focusout", t.scheduleHide);
      delete el.__vTooltip;
    }
    if (tipHost === el) hideTip();
  },
};

/* ---------- lifecycle ---------- */

function isTauri() {
  return "__TAURI_INTERNALS__" in window;
}

async function applyOpenedFile(payload) {
  if (!payload || typeof payload.path !== "string") return;
  const existing = tabs.value.find((t) => t.path === payload.path);
  if (existing) {
    existing.content = payload.content;
    existing.dirty = false;
    await activateTab(existing.id);
  } else {
    const tab = makeTab(payload.path, payload.content);
    tabs.value.push(tab);
    await activateTab(tab.id);
  }
  currentPath.value = payload.path;
  dirty.value = false;
  try {
    localStorage.setItem("cm:content", payload.content);
  } catch {}
  showToast("Opened: " + payload.path.split(/[/\\]/).pop());
}

function onWindowContextMenu(e) {
  // If right-clicked on an input or textarea, allow native cut/copy/paste
  if (e.target.closest("input, textarea")) {
    return;
  }
  // Otherwise disable default browser context menu (back, forward, reload, inspect)
  e.preventDefault();
}

function onWindowClick(e) {
  if (tabContextMenu.value.show) {
    closeTabContextMenu();
  }
}

onMounted(async () => {
  document.documentElement.dataset.theme = theme.value;
  window.addEventListener("keydown", onKeydown);
  window.addEventListener("click", onWindowClick);
  window.addEventListener("contextmenu", onWindowContextMenu);
  document.addEventListener("wheel", onWheel, { passive: false });
  document.addEventListener("scroll", hideTip, true);
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    listen("cm:open-file", (ev) => applyOpenedFile(ev.payload));
    try {
      const startup = await invoke("take_startup_file");
      if (startup) applyOpenedFile(startup);
    } catch {}
    setTimeout(() => checkForUpdates(true), 4000);
  }
  if (previewTarget.value) {
    toc.value = buildToc(previewTarget.value);
    await nextTick();
    await renderMermaidBlocks();
    attachScrollspy();
  }
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("click", onWindowClick);
  window.removeEventListener("contextmenu", onWindowContextMenu);
  document.removeEventListener("wheel", onWheel);
  document.removeEventListener("scroll", hideTip, true);
  observer?.disconnect();
  clearTimeout(debounceTimer);
  clearTimeout(toastTimer);
});
</script>

<template>
  <div class="app" :data-theme="theme">
    <header class="toolbar">
      <div class="brand">
        <svg class="brand-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="3" width="18" height="18" rx="4" />
          <path d="M7 15V9l3 3.5L13 9v6" />
          <path d="M17 9v6" />
          <path d="M15 13.5l2 2 2-2" />
        </svg>
        Chill Markdown
      </div>

      <div class="toolbar-group">
        <button class="btn icon-btn" v-tooltip="'Open markdown file'" aria-label="Open" @click="openFile">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
          </svg>
        </button>
        <button class="btn icon-btn" :disabled="!activeTab" v-tooltip="'Save (Ctrl+S)'" :class="{ primary: dirty }" aria-label="Save" @click="saveFile">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 3h11l5 5v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />
            <path d="M8 3v5h7V3" /><path d="M8 21v-7h8v7" />
          </svg>
        </button>
        <button class="btn icon-btn" :disabled="!activeTab" v-tooltip="'Save as new file'" aria-label="Save as" @click="saveFileAs">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
            <path d="M14 3v5h5" />
            <path d="M12 11.5v5M9.5 14h5" />
          </svg>
        </button>
      </div>

      <div class="toolbar-group">
        <button class="btn icon-btn" :disabled="!activeTab" :class="{ active: editing }" v-tooltip="editing ? 'Exit editor (Esc)' : 'Edit mode (Esc to exit)'" aria-label="Edit" @click="toggleEdit">
          <svg v-if="!editing" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M15.2 4.8l4 4L8 20H4v-4z" /><path d="M12.5 7.5l4 4" />
          </svg>
          <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 13l4 4L19 7" />
          </svg>
        </button>
        <template v-if="editing">
          <button class="btn icon-btn" :disabled="!canUndo" v-tooltip="'Undo (Ctrl+Z)'" aria-label="Undo" @click="undo">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 7v6h6" />
              <path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13" />
            </svg>
          </button>
          <button class="btn icon-btn" :disabled="!canRedo" v-tooltip="'Redo (Ctrl+Y)'" aria-label="Redo" @click="redo">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M21 7v6h-6" />
              <path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3L21 13" />
            </svg>
          </button>
        </template>
        <button class="btn icon-btn" :disabled="!activeTab || exporting" v-tooltip="'Export to PDF (save dialog, defaults to Documents folder)'" aria-label="Export PDF" @click="exportPdf">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
            <path d="M14 3v5h5" />
            <text x="9" y="17" font-size="5.5" font-weight="700" stroke="none" fill="currentColor">PDF</text>
          </svg>
        </button>
        <button class="btn icon-btn" :disabled="!activeTab" :class="{ active: showSearch }" v-tooltip="'Find in document (Ctrl+F)'" aria-label="Find" @click="toggleSearch">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="11" cy="11" r="8" />
            <path d="M21 21l-4.35-4.35" />
          </svg>
        </button>
      </div>

      <div class="toolbar-group zoom">
        <button class="btn icon-btn" :disabled="!activeTab || !canZoomOut" v-tooltip="'Zoom out (Ctrl+−)'" aria-label="Zoom out" @click="zoomOut">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M5 12h14" /></svg>
        </button>
        <button class="btn zoom-label" :disabled="!activeTab" v-tooltip="'Reset zoom (Ctrl+0)'" aria-label="Zoom level, click to reset" @click="zoomReset">{{ zoomPercent }}</button>
        <button class="btn icon-btn" :disabled="!activeTab || !canZoomIn" v-tooltip="'Zoom in (Ctrl++)'" aria-label="Zoom in" @click="zoomIn">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
        </button>
        <button class="btn icon-btn" :disabled="!activeTab" v-tooltip="'File properties'" aria-label="File properties" @click="openProperties">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="9" />
            <path d="M12 11v5" /><circle cx="12" cy="7.6" r="0.4" fill="currentColor" stroke="none" />
          </svg>
        </button>
      </div>

      <div class="spacer"></div>

      <button
        v-if="updateInfo"
        class="btn update-btn"
        :class="{ primary: !updating }"
        :disabled="updating"
        v-tooltip="updating ? 'Installing update…' : (updateInfo.notes || 'Download and install the update, then the app restarts')"
        aria-label="Install update"
        @click="installUpdate"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 3v12" /><path d="M7 11l5 5 5-5" /><path d="M4 19h16" />
        </svg>
        <span>{{ updating ? "Updating…" : "Update v" + updateInfo.version }}</span>
      </button>

      <button
        class="btn icon-btn"
        :class="{ 'is-checking': checkingUpdate }"
        :disabled="checkingUpdate"
        v-tooltip="checkingUpdate ? 'Checking for updates…' : 'Check for updates (Ctrl+U)'"
        aria-label="Check for updates"
        @click="checkForUpdates(false)"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M4.5 16.5c-1.5 1.26-2.5 3.19-2.5 5.5h20c0-2.31-1-4.24-2.5-5.5" />
          <path d="M12 2v10" />
          <path d="M8.5 8.5L12 12l3.5-3.5" />
          <circle cx="12" cy="12" r="9" stroke-dasharray="4 3" />
        </svg>
      </button>

      <button
        class="btn icon-btn"
        :class="{ active: showSidebar }"
        v-tooltip="showSidebar ? 'Hide outline (Ctrl+B)' : 'Show outline (Ctrl+B)'"
        aria-label="Toggle outline"
        @click="toggleSidebar"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="3" width="18" height="18" rx="2" />
          <path d="M15 3v18" />
        </svg>
      </button>

      <button
        class="btn icon-btn"
        v-tooltip="theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'"
        :aria-label="theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'"
        @click="theme = theme === 'dark' ? 'light' : 'dark'"
      >
        <svg v-if="theme === 'dark'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
        </svg>
        <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12.8A8.5 8.5 0 1 1 11.2 3a6.5 6.5 0 0 0 9.8 9.8z" />
        </svg>
      </button>

      <button class="btn icon-btn" v-tooltip="'About Chill Markdown'" aria-label="About" @click="openAbout">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="9" />
          <path d="M9.6 9.2a2.5 2.5 0 1 1 3.6 2.26c-.72.4-1.2.94-1.2 1.74v.3" />
          <circle cx="12" cy="16.6" r="0.5" fill="currentColor" stroke="none" />
        </svg>
      </button>
    </header>

    <div v-if="tabs.length" class="tabstrip" role="tablist" aria-label="Open files">
      <div
        v-for="tab in tabs"
        :key="tab.id"
        class="file-tab"
        :class="{ active: tab.id === activeTabId }"
        role="tab"
        :aria-selected="tab.id === activeTabId"
        :title="tab.path || tabLabel(tab)"
        tabindex="0"
        @click="activateTab(tab.id)"
        @auxclick="onTabAuxClick(tab.id, $event)"
        @contextmenu="onTabContextMenu(tab.id, $event)"
        @keydown.enter="activateTab(tab.id)"
        v-tooltip="tab.path || tabLabel(tab)"
      >
        <svg class="tab-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" /><path d="M14 3v5h5" />
        </svg>
        <span class="tab-name">{{ tabLabel(tab) }}</span>
        <span v-if="tab.dirty" class="dirty">•</span>
        <button
          class="tab-close"
          v-tooltip="'Close tab (Ctrl+W)'"
          aria-label="Close tab"
          @click.stop="closeTab(tab.id)"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
        </button>
      </div>
      <button class="btn icon-btn new-tab-btn" v-tooltip="'New tab (Ctrl+T)'" aria-label="New tab" @click="newTab">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
      </button>
    </div>

    <main
      class="workspace"
      :class="{
        'split-dragging': isDraggingSplit || isDraggingSidebar,
        'has-search': showSearch
      }"
    >
      <div v-if="showSearch" class="search-panel">
        <div class="search-row">
          <button
            v-if="editing"
            class="btn icon-btn search-toggle-btn"
            :class="{ active: showReplace }"
            v-tooltip="showReplace ? 'Hide replace' : 'Toggle replace (Ctrl+H)'"
            aria-label="Toggle replace"
            @click="toggleReplace"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M6 9l6 6 6-6" />
            </svg>
          </button>

          <div class="search-input-wrap">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="11" cy="11" r="8" /><path d="M21 21l-4.35-4.35" />
            </svg>
            <input
              ref="searchInputRef"
              v-model="searchQuery"
              type="text"
              class="search-input"
              placeholder="Find in document…"
              @input="executeSearch(false)"
              @keydown.enter.exact="findNext"
              @keydown.shift.enter.exact="findPrev"
              @keydown.esc="closeSearch"
            />
          </div>

          <div class="search-counter">
            {{ searchTotal > 0 ? `${searchIndex} of ${searchTotal}` : (searchQuery ? '0 matches' : '') }}
          </div>

          <button
            class="btn icon-btn search-opt-btn"
            :class="{ active: searchCaseSensitive }"
            v-tooltip="'Match case'"
            aria-label="Match case"
            @click="searchCaseSensitive = !searchCaseSensitive; executeSearch(false)"
          >
            Aa
          </button>

          <button
            class="btn icon-btn"
            :disabled="searchTotal === 0"
            v-tooltip="'Previous match (Shift+Enter)'"
            aria-label="Previous match"
            @click="findPrev"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 15l-6-6-6 6" /></svg>
          </button>

          <button
            class="btn icon-btn"
            :disabled="searchTotal === 0"
            v-tooltip="'Next match (Enter)'"
            aria-label="Next match"
            @click="findNext"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6" /></svg>
          </button>

          <button
            class="btn icon-btn"
            v-tooltip="'Close (Esc)'"
            aria-label="Close find"
            @click="closeSearch"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
          </button>
        </div>

        <!-- Replace row in Editor mode -->
        <div v-if="editing && showReplace" class="search-row replace-row">
          <div class="search-input-wrap">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M17 3v4a2 2 0 0 1-2 2H5" /><path d="M9 13l-4-4 4-4" />
              <path d="M7 21v-4a2 2 0 0 1 2-2h10" /><path d="M15 11l4 4-4 4" />
            </svg>
            <input
              ref="replaceInputRef"
              v-model="replaceQuery"
              type="text"
              class="search-input"
              placeholder="Replace with…"
              @keydown.enter.exact="replaceCurrent"
              @keydown.esc="closeSearch"
            />
          </div>

          <button
            class="btn search-action-btn"
            :disabled="searchTotal === 0"
            v-tooltip="'Replace current match'"
            @click="replaceCurrent"
          >
            Replace
          </button>

          <button
            class="btn search-action-btn"
            :disabled="searchTotal === 0"
            v-tooltip="'Replace all matches'"
            @click="replaceAll"
          >
            Replace all
          </button>
        </div>
      </div>

      <!-- Empty state when no tabs are open -->
      <div v-if="!activeTab" class="empty-workspace">
        <div class="empty-card">
          <svg class="empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <rect x="3" y="3" width="18" height="18" rx="4" />
            <path d="M7 15V9l3 3.5L13 9v6" />
            <path d="M17 9v6" />
            <path d="M15 13.5l2 2 2-2" />
          </svg>
          <div class="empty-title">Chill Markdown</div>
          <div class="empty-sub">No files open. Create a new document or open an existing markdown file to get started.</div>
          <div class="empty-actions">
            <button class="btn primary empty-btn" @click="newTab">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
              <span>New Document</span>
            </button>
            <button class="btn empty-btn" @click="openFile">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
              </svg>
              <span>Open File…</span>
            </button>
          </div>
        </div>
      </div>

      <template v-else>
        <div ref="splitAreaRef" class="split-area">
          <textarea
            v-if="editing"
            ref="editorRef"
            v-model="content"
            class="editor"
            :style="[editorStyle, { fontSize: 12 * zoom + 'px' }]"
            spellcheck="false"
            placeholder="Write markdown here…"
            @keydown="onEditorKeydown"
          ></textarea>
          <div
            v-if="editing"
            class="split-divider"
            :class="{ active: isDraggingSplit }"
            role="separator"
            aria-orientation="vertical"
            :aria-valuenow="Math.round(splitRatio * 100)"
            aria-label="Editor split size — drag, use Left/Right arrows, or double-click to reset"
            v-tooltip="'Drag to resize the editor pane'"
            tabindex="0"
            @pointerdown="onSplitPointerDown"
            @pointermove="onSplitPointerMove"
            @pointerup="onSplitPointerUp"
            @pointercancel="onSplitPointerUp"
            @dblclick="resetSplit"
            @keydown="onSplitKeydown"
          ></div>
          <div
            ref="previewRef"
            class="md-content md-preview"
            :style="{ fontSize: 12 * zoom + 'px' }"
            v-html="previewHtml"
            @click="onPreviewClick"
          ></div>
        </div>

        <!-- Resizable Sidebar TOC Divider -->
        <div
          v-if="showSidebar"
          class="sidebar-divider"
          :class="{ active: isDraggingSidebar }"
          role="separator"
          aria-orientation="vertical"
          :aria-valuenow="sidebarWidth"
          aria-label="Sidebar outline size — drag or double-click to reset"
          v-tooltip="'Drag to resize outline sidebar'"
          tabindex="0"
          @pointerdown="onSidebarPointerDown"
          @pointermove="onSidebarPointerMove"
          @pointerup="onSidebarPointerUp"
          @pointercancel="onSidebarPointerUp"
          @dblclick="resetSidebarWidth"
          @keydown="onSidebarKeydown"
        ></div>

        <aside
          v-if="showSidebar"
          class="toc"
          :style="{ width: sidebarWidth + 'px' }"
        >
          <div class="toc-header">
            <div class="toc-title">Contents</div>
            <button
              class="btn icon-btn toc-close-btn"
              v-tooltip="'Hide outline (Ctrl+B)'"
              aria-label="Hide outline"
              @click="toggleSidebar"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
            </button>
          </div>
          <nav class="toc-list">
            <template v-if="toc.length">
              <a
                v-for="item in toc"
                :key="item.id"
                :class="['toc-item', `lvl-${item.level}`, { active: activeId === item.id }]"
                :href="`#${item.id}`"
                @click.prevent="scrollToHeading(item.id)"
              >
                {{ item.text }}
              </a>
            </template>
            <div v-else class="toc-empty">No headings yet</div>
          </nav>
        </aside>
      </template>
    </main>

    <transition name="modal">
      <div v-if="showProps" class="modal-backdrop" @click.self="showProps = false">
        <div class="modal" role="dialog" aria-label="File properties">
          <header class="modal-head">
            <div class="modal-title">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="9" /><path d="M12 11v5" /><circle cx="12" cy="7.6" r="0.4" fill="currentColor" stroke="none" />
              </svg>
              File properties
            </div>
            <button class="btn icon-btn" v-tooltip="'Close (Esc)'" aria-label="Close" @click="showProps = false">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
            </button>
          </header>
          <div class="modal-body">
            <div v-if="propsError" class="modal-error">{{ propsError }}</div>
            <dl class="props-list">
              <template v-for="([label, value]) in propsRows" :key="label">
                <dt>{{ label }}</dt>
                <dd :class="{ mono: label === 'Path', muted: String(value).startsWith('—') }">{{ value }}</dd>
              </template>
            </dl>
          </div>
          <footer class="modal-foot">
            <span class="modal-hint">Press Esc or click outside to close</span>
          </footer>
        </div>
      </div>
    </transition>

    <transition name="modal">
      <div v-if="showAbout" class="modal-backdrop" @click.self="showAbout = false">
        <div class="modal about-modal" role="dialog" aria-label="About Chill Markdown">
          <header class="modal-head">
            <div class="modal-title">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="9" />
                <path d="M9.6 9.2a2.5 2.5 0 1 1 3.6 2.26c-.72.4-1.2.94-1.2 1.74v.3" />
                <circle cx="12" cy="16.6" r="0.5" fill="currentColor" stroke="none" />
              </svg>
              About
            </div>
            <button class="btn icon-btn" v-tooltip="'Close (Esc)'" aria-label="Close" @click="showAbout = false">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
            </button>
          </header>
          <div class="modal-body">
            <div class="about-brand">
              <div class="about-logo">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <rect x="2.5" y="2.5" width="19" height="19" rx="4.5" />
                  <path d="M6.5 15.5V8.5l3.5 4 3.5-4v7" />
                  <path d="M17.5 8.5v7" />
                  <path d="M15 13l2.5 2.5 2.5-2.5" />
                </svg>
              </div>
              <div class="about-name-wrap">
                <span class="about-name">Chill Markdown</span>
                <span class="about-version">v{{ appVersion }}</span>
              </div>
            </div>
            <p class="about-desc">
              A calm markdown viewer &amp; editor — distraction-free reading, live preview,
              mermaid diagrams, LaTeX math, PDF export, and auto updates.
            </p>
            <ul class="about-links">
              <li>
                <button class="about-link" @click="openGithub">
                  <svg viewBox="0 0 24 24" fill="currentColor" stroke="none"><path d="M12 2C6.48 2 2 6.58 2 12.25c0 4.53 2.87 8.37 6.84 9.73.5.09.68-.22.68-.49v-1.72c-2.78.62-3.37-1.37-3.37-1.37-.45-1.18-1.11-1.5-1.11-1.5-.91-.64.07-.63.07-.63 1 .07 1.53 1.06 1.53 1.06.9 1.57 2.36 1.12 2.94.86.09-.67.35-1.12.63-1.38-2.22-.26-4.56-1.14-4.56-5.07 0-1.12.39-2.03 1.03-2.75-.1-.26-.45-1.3.1-2.7 0 0 .84-.28 2.75 1.05a9.36 9.36 0 0 1 5 0c1.91-1.33 2.75-1.05 2.75-1.05.55 1.4.2 2.44.1 2.7.64.72 1.03 1.63 1.03 2.75 0 3.94-2.34 4.8-4.57 5.06.36.32.68.94.68 1.9v2.81c0 .27.18.59.69.49A10.05 10.05 0 0 0 22 12.25C22 6.58 17.52 2 12 2z"/></svg>
                  Source code &amp; docs
                </button>
              </li>
              <li>
                <button class="about-link" @click="openIssues">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9" /><path d="M12 8v5" /><circle cx="12" cy="15.9" r="0.5" fill="currentColor" stroke="none" /></svg>
                  Report an issue
                </button>
              </li>
              <li>
                <button class="about-link about-link-highlight" @click="openBmc">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M4 8h12v6a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4z" />
                    <path d="M16 9.5h1.5a2.5 2.5 0 0 1 0 5H16" />
                    <path d="M6.5 4.5c0 .8.5.7.5 1.3M9.5 4.5c0 .8.5.7.5 1.3M12.5 4.5c0 .8.5.7.5 1.3" />
                    <path d="M5 20.5h10" />
                  </svg>
                  Buy me a coffee
                </button>
              </li>
              <li>
                <button class="about-link" :disabled="checkingUpdate" @click="checkForUpdates(false)">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <circle cx="12" cy="12" r="9" stroke-dasharray="4 3" />
                    <path d="M12 7v6" />
                    <path d="M9 10.5L12 13.5l3-3.5" />
                  </svg>
                  {{ checkingUpdate ? "Checking for updates…" : "Check for updates" }}
                </button>
              </li>
              <li class="about-meta">
                <span class="about-meta-item">License: Apache-2.0</span>
                <span class="about-meta-item">Find: Ctrl/Cmd + F</span>
                <span class="about-meta-item">Toggle outline: Ctrl/Cmd + B</span>
                <span class="about-meta-item">Undo/Redo: Ctrl/Cmd + Z / Y</span>
              </li>
            </ul>
          </div>
          <footer class="modal-foot">
            <span class="modal-hint">Built with Tauri + Vue 3 · Keep it chill</span>
          </footer>
        </div>
      </div>
    </transition>

    <!-- PDF Export Progress Modal -->
    <transition name="modal">
      <div v-if="exporting" class="modal-backdrop">
        <div class="modal export-modal" role="dialog" aria-label="Exporting PDF">
          <div class="export-spinner-wrap">
            <svg class="export-spinner" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
              <circle cx="12" cy="12" r="9" stroke-dasharray="36" stroke-dashoffset="18" />
            </svg>
            <div class="export-modal-title">Mengekspor ke PDF</div>
            <div class="export-modal-desc">{{ exportProgress || "Memproses dokumen…" }}</div>
            <div class="export-bar-track">
              <div class="export-bar-fill" :style="{ width: exportPercent + '%' }"></div>
            </div>
            <div class="export-percent-label">{{ exportPercent }}%</div>
          </div>
        </div>
      </div>
    </transition>

    <!-- PDF Export Success Modal -->
    <transition name="modal">
      <div v-if="exportResultModal.show" class="modal-backdrop" @click.self="exportResultModal.show = false">
        <div class="modal export-modal" role="dialog" aria-label="PDF Exported Successfully">
          <header class="modal-head">
            <div class="modal-title">
              <svg viewBox="0 0 24 24" fill="none" stroke="#22c55e" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="9" />
                <path d="M8.5 12.5l2.5 2.5 5-5" />
              </svg>
              PDF Exported
            </div>
            <button class="btn icon-btn" v-tooltip="'Close (Esc)'" aria-label="Close" @click="exportResultModal.show = false">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
            </button>
          </header>
          <div class="modal-body export-success-body">
            <p class="export-success-msg">
              Dokumen berhasil diekspor ke format PDF:
            </p>
            <div class="export-filename-box mono">{{ exportResultModal.fileName }}</div>
          </div>
          <footer class="modal-foot export-modal-actions">
            <button class="btn primary export-btn" @click="openExportedPdf(); exportResultModal.show = false">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                <path d="M15 3h6v6" /><path d="M10 14L21 3" />
              </svg>
              <span>Buka File</span>
            </button>
            <button class="btn export-btn" @click="showExportedInFolder(); exportResultModal.show = false">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
              </svg>
              <span>Buka Folder</span>
            </button>
            <button class="btn export-btn" @click="exportResultModal.show = false">
              Tutup
            </button>
          </footer>
        </div>
      </div>
    </transition>

    <transition name="toast">
      <div v-if="toast" class="toast">{{ toast }}</div>
    </transition>

    <!-- Tab Context Menu -->
    <div
      v-if="tabContextMenu.show"
      class="context-menu"
      :style="{ top: tabContextMenu.y + 'px', left: tabContextMenu.x + 'px' }"
      @click.stop
    >
      <button class="context-item" @click="closeTab(tabContextMenu.tabId); closeTabContextMenu()">
        Close tab
      </button>
      <button class="context-item" @click="closeOtherTabs(tabContextMenu.tabId); closeTabContextMenu()">
        Close other tabs
      </button>
      <button class="context-item" @click="closeTabsToRight(tabContextMenu.tabId); closeTabContextMenu()">
        Close tabs to the right
      </button>
      <button class="context-item" @click="closeTabsToLeft(tabContextMenu.tabId); closeTabContextMenu()">
        Close tabs to the left
      </button>
    </div>
  </div>
</template>

<style>
:root {
  --bg: #f6f7f9;
  --panel: #ffffff;
  --panel-alt: #eef0f3;
  --fg: #1c2024;
  --fg-soft: #5a6472;
  --accent: #4c7dd1;
  --border: #d8dde5;
  --code-bg: #f0f2f5;
  --shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
}

html[data-theme="dark"] {
  --bg: #14171c;
  --panel: #1b2027;
  --panel-alt: #232933;
  --fg: #e6eaf0;
  --fg-soft: #9aa4b2;
  --accent: #7aa5e5;
  --border: #2c333d;
  --code-bg: #10141a;
  --shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
}

* {
  box-sizing: border-box;
}

html,
body,
#app {
  height: 100%;
  margin: 0;
  padding: 0;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, system-ui, sans-serif;
  color: var(--fg);
  background: var(--bg);
  overflow: hidden;
}

.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  color: var(--fg);
  background: var(--bg);
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  flex: none;
  user-select: none;
}

.brand {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  font-weight: 700;
  margin-right: 6px;
  white-space: nowrap;
}

.brand-icon {
  width: 17px;
  height: 17px;
  color: var(--accent);
}

.toolbar-group {
  display: flex;
  gap: 4px;
}

.toolbar .spacer {
  flex: 1;
}

.dirty {
  color: #e5933c;
  font-weight: 700;
}

.btn {
  border: 1px solid var(--border);
  background: var(--panel);
  color: var(--fg);
  font-size: 13px;
  padding: 6px 10px;
  border-radius: 7px;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s;
}

.btn:hover:not(:disabled) {
  border-color: var(--accent);
  background: var(--panel-alt);
}

.btn:disabled {
  opacity: 0.45;
  cursor: default;
}

.btn.active {
  border-color: var(--accent);
  color: var(--accent);
  font-weight: 600;
}

.btn.primary {
  border-color: var(--accent);
  color: #fff;
  background: var(--accent);
}

.btn.primary:hover:not(:disabled) {
  border-color: var(--accent);
  background: var(--accent);
  color: #fff;
  filter: brightness(1.08);
}

.btn.icon-btn {
  width: 34px;
  padding: 6px 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.btn.icon-btn svg {
  width: 16px;
  height: 16px;
  display: block;
}

.btn.zoom-label {
  min-width: 52px;
  font-variant-numeric: tabular-nums;
}

.btn.update-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.btn.update-btn svg {
  width: 15px;
  height: 15px;
}

.btn.is-checking svg {
  animation: cm-spin 1.2s linear infinite;
  transform-origin: center;
}

@keyframes cm-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.workspace {
  display: flex;
  flex: 1;
  min-height: 0;
}

.split-area {
  display: flex;
  flex: 1;
  min-width: 0;
  min-height: 0;
}

/* ---------- empty workspace state ---------- */

.empty-workspace {
  display: flex;
  flex: 1;
  align-items: center;
  justify-content: center;
  padding: 40px 20px;
  background: var(--bg);
}

.empty-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  max-width: 440px;
}

.empty-icon {
  width: 56px;
  height: 56px;
  color: var(--accent);
  margin-bottom: 16px;
  opacity: 0.9;
}

.empty-title {
  font-size: 22px;
  font-weight: 700;
  color: var(--fg);
  margin-bottom: 8px;
}

.empty-sub {
  font-size: 14px;
  line-height: 1.5;
  color: var(--fg-soft);
  margin-bottom: 24px;
}

.empty-actions {
  display: flex;
  gap: 12px;
  align-items: center;
}

.empty-btn {
  padding: 8px 18px;
  font-size: 14px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.empty-btn svg {
  width: 16px;
  height: 16px;
}

/* ---------- tab strip ---------- */

.tabstrip {
  display: flex;
  align-items: stretch;
  gap: 2px;
  padding: 4px 8px 0;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  overflow-x: auto;
  scroll-behavior: smooth;
  scrollbar-width: thin;
  flex: none;
}

.tabstrip::after {
  content: "";
  flex: 1;
  min-width: 4px;
  border-bottom: 1px solid transparent;
}

.file-tab {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 7px 8px 7px 12px;
  border: 1px solid transparent;
  border-bottom: none;
  border-radius: 9px 9px 0 0;
  color: var(--fg-soft);
  cursor: pointer;
  max-width: 220px;
  white-space: nowrap;
  transition: background 0.15s, color 0.15s;
}

.file-tab:hover {
  background: var(--panel-alt);
  color: var(--fg);
}

.file-tab.active {
  color: var(--fg);
  background: var(--bg);
  border-color: var(--border);
  font-weight: 600;
}

.tab-icon {
  width: 14px;
  height: 14px;
  flex: none;
  opacity: 0.75;
}

.tab-name {
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 12.5px;
}

.file-tab .dirty {
  font-size: 15px;
  line-height: 1;
  color: #e5933c;
}

.tab-close {
  flex: none;
  width: 17px;
  height: 17px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: inherit;
  border-radius: 4px;
  cursor: pointer;
  opacity: 0.55;
  padding: 0;
}

.tab-close svg {
  width: 11px;
  height: 11px;
}

.tab-close:hover {
  opacity: 1;
  background: var(--panel-alt);
  color: var(--fg);
}

.new-tab-btn {
  align-self: center;
  margin-left: 4px;
  height: 26px;
  width: 28px;
  padding: 0;
}

.workspace.split-dragging {
  cursor: col-resize;
  user-select: none;
}

.workspace.split-dragging .md-preview,
.workspace.split-dragging .editor,
.workspace.split-dragging .toc {
  pointer-events: none;
}

.split-divider,
.sidebar-divider {
  flex: none;
  width: 5px;
  margin: 4px 2px;
  border-radius: 3px;
  background: var(--border);
  cursor: col-resize;
  touch-action: none;
  transition: background 0.15s;
  outline: none;
}

.split-divider:hover,
.split-divider.active,
.sidebar-divider:hover,
.sidebar-divider.active {
  background: var(--accent);
}

.split-divider:focus-visible,
.sidebar-divider:focus-visible {
  box-shadow: inset 0 0 0 2px var(--panel), inset 0 0 0 4px var(--accent);
}

.editor {
  flex: 1 1 38%;
  min-width: 0;
  border: none;
  outline: none;
  resize: none;
  padding: 20px 24px;
  background: var(--panel);
  color: var(--fg);
  font-family: "SF Mono", Menlo, Consolas, "JetBrains Mono", ui-monospace, monospace;
  font-size: 12px;
  line-height: 1.6;
  tab-size: 2;
}

.md-preview {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: 28px 40px 80px;
  background: var(--bg);
  scroll-behavior: smooth;
}

/* ---------- markdown typography (semua em, zoom !==> font-size root) ---------- */

.md-content {
  color: var(--fg);
  line-height: 1.65;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, system-ui, sans-serif;
}

.md-content h1,
.md-content h2,
.md-content h3,
.md-content h4,
.md-content h5,
.md-content h6 {
  margin: 1.4em 0 0.55em;
  font-weight: 700;
  line-height: 1.3;
  scroll-margin-top: 12px;
}

.md-content h1 {
  font-size: 1.9em;
  border-bottom: 2px solid var(--border);
  padding-bottom: 0.3em;
}

.md-content h2 {
  font-size: 1.5em;
  border-bottom: 1px solid var(--border);
  padding-bottom: 0.25em;
}

.md-content h3 {
  font-size: 1.25em;
}

.md-content h4 {
  font-size: 1.1em;
}

.md-content p {
  margin: 0.7em 0;
}

.md-content a {
  color: var(--accent);
  text-decoration: none;
}

.md-content a:hover {
  text-decoration: underline;
}

.md-content ul,
.md-content ol {
  margin: 0.7em 0;
  padding-left: 1.6em;
}

.md-content li {
  margin: 0.3em 0;
}

.md-content blockquote {
  margin: 0.9em 0;
  padding: 0.4em 1em;
  border-left: 4px solid var(--accent);
  background: var(--panel);
  color: var(--fg-soft);
  border-radius: 0 6px 6px 0;
}

.md-content hr {
  border: none;
  border-top: 1px solid var(--border);
  margin: 1.6em 0;
}

.md-content code,
.md-content pre {
  font-family: "SF Mono", Menlo, Consolas, "JetBrains Mono", ui-monospace, monospace;
  font-size: 0.88em;
}

.md-content :not(pre) > code {
  background: var(--code-bg);
  padding: 0.15em 0.4em;
  border-radius: 5px;
  border: 1px solid var(--border);
}

.md-content pre.hljs {
  background: var(--code-bg);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 1em 1.2em;
  overflow-x: auto;
  line-height: 1.5;
}

.md-content pre.hljs code {
  background: transparent;
  padding: 0;
  border: none;
}

.md-content table {
  border-collapse: collapse;
  margin: 1em 0;
  width: 100%;
}

.md-content th,
.md-content td {
  border: 1px solid var(--border);
  padding: 0.45em 0.75em;
  text-align: left;
}

.md-content th {
  background: var(--panel);
  font-weight: 700;
}

.md-content img {
  max-width: 100%;
  border-radius: 8px;
}

.md-content .katex-display {
  overflow-x: auto;
  overflow-y: hidden;
  padding: 0.4em 0;
}

.md-content .katex-error {
  color: #d05b5b;
}

.md-content .mermaid-block {
  display: flex;
  justify-content: flex-start;
  margin: 1.2em 0;
  overflow-x: auto;
}

.md-content .mermaid-block svg {
  max-width: 100%;
  height: auto;
  font-size: 11px;
}

.md-content .mermaid-block text,
.md-content .mermaid-block .label text,
.md-content .mermaid-block .node text,
.md-content .mermaid-block .commit-label,
.md-content .mermaid-block .branch-label {
  font-size: 11px !important;
}

.md-content .mermaid-hint {
  color: var(--fg-soft);
  font-style: italic;
}

.md-content .mermaid-error {
  color: #d05b5b;
  font-family: ui-monospace, monospace;
  font-size: 0.9em;
}

/* syntax highlighting (token classes hljs, warna diatur per tema) */

.md-content .hljs-keyword,
.md-content .hljs-selector-tag,
.md-content .hljs-literal,
.md-content .hljs-section,
.md-content .hljs-doctag,
.md-content .hljs-type,
.md-content .hljs-name,
.md-content .hljs-strong {
  color: #7c52c7;
  font-weight: 600;
}

.md-content .hljs-string,
.md-content .hljs-regexp,
.md-content .hljs-symbol,
.md-content .hljs-bullet,
.md-content .hljs-addition {
  color: #2c8a54;
}

.md-content .hljs-number,
.md-content .hljs-meta {
  color: #b4690e;
}

.md-content .hljs-comment,
.md-content .hljs-quote,
.md-content .hljs-deletion {
  color: #7a8494;
  font-style: italic;
}

.md-content .hljs-title,
.md-content .hljs-function,
.md-content .hljs-title.function_ {
  color: #2465c4;
}

.md-content .hljs-attr,
.md-content .hljs-attribute,
.md-content .hljs-variable,
.md-content .hljs-template-variable,
.md-content .hljs-property {
  color: #b04848;
}

html[data-theme="dark"] .md-content .hljs-keyword,
html[data-theme="dark"] .md-content .hljs-selector-tag,
html[data-theme="dark"] .md-content .hljs-literal,
html[data-theme="dark"] .md-content .hljs-section,
html[data-theme="dark"] .md-content .hljs-doctag,
html[data-theme="dark"] .md-content .hljs-type,
html[data-theme="dark"] .md-content .hljs-name,
html[data-theme="dark"] .md-content .hljs-strong {
  color: #b39ae6;
}

html[data-theme="dark"] .md-content .hljs-string,
html[data-theme="dark"] .md-content .hljs-regexp,
html[data-theme="dark"] .md-content .hljs-symbol,
html[data-theme="dark"] .md-content .hljs-bullet,
html[data-theme="dark"] .md-content .hljs-addition {
  color: #7dcc9d;
}

html[data-theme="dark"] .md-content .hljs-number,
html[data-theme="dark"] .md-content .hljs-meta {
  color: #e2a45f;
}

html[data-theme="dark"] .md-content .hljs-comment,
html[data-theme="dark"] .md-content .hljs-quote,
html[data-theme="dark"] .md-content .hljs-deletion {
  color: #7d8898;
}

html[data-theme="dark"] .md-content .hljs-title,
html[data-theme="dark"] .md-content .hljs-function,
html[data-theme="dark"] .md-content .hljs-title.function_ {
  color: #7aabf0;
}

html[data-theme="dark"] .md-content .hljs-attr,
html[data-theme="dark"] .md-content .hljs-attribute,
html[data-theme="dark"] .md-content .hljs-variable,
html[data-theme="dark"] .md-content .hljs-template-variable,
html[data-theme="dark"] .md-content .hljs-property {
  color: #e08a8a;
}

/* ---------- search bar & search match highlight ---------- */

.workspace {
  position: relative;
}

.search-panel {
  position: absolute;
  top: 10px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 50;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 6px 8px;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
}

.search-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.search-toggle-btn {
  width: 24px;
  height: 24px;
  padding: 0;
  transition: transform 0.15s;
}

.search-toggle-btn.active svg {
  transform: rotate(90deg);
}

.search-action-btn {
  padding: 3px 8px;
  font-size: 12px;
  height: 26px;
  white-space: nowrap;
}

.search-input-wrap {
  display: flex;
  align-items: center;
  gap: 6px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 4px 8px;
}

.search-input-wrap svg {
  width: 14px;
  height: 14px;
  color: var(--fg-soft);
  flex: none;
}

.search-input {
  border: none;
  background: transparent;
  outline: none;
  color: var(--fg);
  font-size: 13px;
  width: 160px;
}

.search-counter {
  font-size: 11px;
  color: var(--fg-soft);
  padding: 0 4px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.search-opt-btn {
  font-size: 12px;
  font-weight: 700;
  padding: 3px 6px;
  min-width: 26px;
  height: 26px;
}

.search-opt-btn.active {
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
}

mark.cm-search-match {
  background: #fde047;
  color: #000;
  border-radius: 2px;
  padding: 0 1px;
}

mark.cm-search-match.cm-current {
  background: #f97316;
  color: #fff;
  outline: 2px solid #ea580c;
}

html[data-theme="dark"] mark.cm-search-match {
  background: #a16207;
  color: #fef08a;
}

html[data-theme="dark"] mark.cm-search-match.cm-current {
  background: #ea580c;
  color: #fff;
}

/* ---------- toc panel (kanan) ---------- */

.toc {
  flex: none;
  background: var(--panel);
  border-left: 1px solid var(--border);
  overflow-y: auto;
  padding: 10px 0 24px;
}

.toc-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 12px 10px 16px;
}

.toc-title {
  font-size: 12px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--fg-soft);
  padding: 0;
}

.toc-close-btn {
  width: 24px;
  height: 24px;
  padding: 0;
}

.toc-close-btn svg {
  width: 14px;
  height: 14px;
}

.toc-list {
  display: flex;
  flex-direction: column;
}

.toc-item {
  display: block;
  color: var(--fg-soft);
  text-decoration: none;
  font-size: 13px;
  line-height: 1.4;
  padding: 5px 16px;
  border-left: 3px solid transparent;
  cursor: pointer;
}

.toc-item:hover {
  background: var(--panel-alt);
  color: var(--fg);
}

.toc-item.active {
  color: var(--accent);
  border-left-color: var(--accent);
  font-weight: 600;
  background: var(--panel-alt);
}

.toc-item.lvl-2 {
  padding-left: 26px;
}

.toc-item.lvl-3 {
  padding-left: 36px;
}

.toc-item.lvl-4,
.toc-item.lvl-5,
.toc-item.lvl-6 {
  padding-left: 46px;
  font-size: 12px;
}

.toc-empty {
  color: var(--fg-soft);
  font-size: 12px;
  padding: 4px 16px;
  font-style: italic;
}

/* ---------- file properties modal ---------- */

.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(10, 14, 20, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 40;
}

.modal {
  width: min(460px, 90vw);
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  background: var(--panel);
  color: var(--fg);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
  overflow: hidden;
}

.modal-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 14px 12px 16px;
  border-bottom: 1px solid var(--border);
}

.modal-title {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-weight: 700;
  font-size: 14px;
}

.modal-title svg {
  width: 16px;
  height: 16px;
  color: var(--accent);
}

.modal-body {
  padding: 10px 16px;
  overflow-y: auto;
}

.modal-error {
  color: #d05b5b;
  font-size: 12.5px;
  margin-bottom: 6px;
  word-break: break-all;
}

.props-list {
  display: grid;
  grid-template-columns: 130px 1fr;
  gap: 7px 12px;
  margin: 8px 0;
}

.props-list dt {
  color: var(--fg-soft);
  font-size: 12.5px;
  padding-top: 1px;
}

.props-list dd {
  margin: 0;
  font-size: 13px;
  word-break: break-word;
}

.props-list dd.mono {
  font-family: "SF Mono", Menlo, Consolas, ui-monospace, monospace;
  font-size: 12px;
}

.props-list dd.muted {
  color: var(--fg-soft);
  font-style: italic;
}

.modal-foot {
  padding: 8px 16px;
  border-top: 1px solid var(--border);
}

.modal-hint {
  font-size: 11.5px;
  color: var(--fg-soft);
}

/* ---------- about modal ---------- */

.about-modal .modal-body {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding-top: 16px;
}

/* ---------- export modal ---------- */

.export-modal {
  max-width: 380px;
}

.export-spinner-wrap {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px 24px;
  text-align: center;
}

.export-spinner {
  width: 42px;
  height: 42px;
  color: var(--accent);
  animation: cm-spin 1.2s linear infinite;
  margin-bottom: 16px;
}

.export-modal-title {
  font-size: 16px;
  font-weight: 700;
  color: var(--fg);
  margin-bottom: 6px;
}

.export-modal-desc {
  font-size: 13px;
  color: var(--fg-soft);
  margin-bottom: 14px;
}

.export-bar-track {
  width: 100%;
  max-width: 280px;
  height: 6px;
  background: var(--border);
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 6px;
}

.export-bar-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 4px;
  transition: width 0.25s ease;
}

.export-percent-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--accent);
  font-variant-numeric: tabular-nums;
}

.export-success-body {
  padding: 16px 20px 8px;
}

.export-success-msg {
  font-size: 13.5px;
  color: var(--fg);
  margin: 0 0 10px;
}

.export-filename-box {
  background: var(--bg);
  border: 1px solid var(--border);
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 12.5px;
  word-break: break-all;
  color: var(--accent);
  font-weight: 600;
}

.export-modal-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding: 14px 20px;
}

.export-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  font-size: 13px;
  line-height: 1;
}

.export-btn svg {
  width: 15px;
  height: 15px;
  flex: none;
}

.about-brand {
  display: flex;
  align-items: center;
  gap: 12px;
}

.about-logo {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  color: var(--accent);
}

.about-logo svg {
  width: 100%;
  height: 100%;
}

.about-name-wrap {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.about-name {
  font-size: 18px;
  font-weight: 700;
  letter-spacing: 0.2px;
}

.about-version {
  display: inline-flex;
  align-self: flex-start;
  padding: 1px 9px;
  border-radius: 999px;
  border: 1px solid var(--border);
  background: var(--panel-alt);
  color: var(--fg-soft);
  font-size: 11.5px;
  font-weight: 600;
}

.about-desc {
  margin: 0;
  font-size: 13px;
  line-height: 1.55;
  color: var(--fg-soft);
}

.about-links {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.about-link {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--panel-alt);
  color: var(--fg);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s, color 0.15s;
}

.about-link:hover {
  background: var(--bg);
  border-color: var(--accent);
  color: var(--accent);
}

.about-link-highlight:hover {
  border-color: #e5933c;
  color: #e5933c;
}

.about-link svg {
  width: 17px;
  height: 17px;
  flex: none;
}

.about-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 14px;
  margin-top: 2px;
}

.about-meta-item {
  font-size: 11.5px;
  color: var(--fg-soft);
}

.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.18s ease;
}

.modal-enter-active .modal,
.modal-leave-active .modal {
  transition: transform 0.18s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from .modal,
.modal-leave-to .modal {
  transform: translateY(10px) scale(0.98);
}

/* ---------- toast ---------- */

.toast {
  position: fixed;
  bottom: 18px;
  left: 50%;
  transform: translateX(-50%);
  background: var(--fg);
  color: var(--bg);
  font-size: 13px;
  padding: 8px 16px;
  border-radius: 8px;
  box-shadow: var(--shadow);
  z-index: 50;
  max-width: 70%;
}

.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.2s, transform 0.2s;
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}

/* ---------- tab context menu ---------- */

.context-menu {
  position: fixed;
  z-index: 100;
  min-width: 170px;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 4px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.16);
  display: flex;
  flex-direction: column;
}

.context-item {
  border: none;
  background: transparent;
  color: var(--fg);
  font-size: 13px;
  text-align: left;
  padding: 6px 12px;
  border-radius: 5px;
  cursor: pointer;
  transition: background 0.1s, color 0.1s;
}

.context-item:hover {
  background: var(--panel-alt);
  color: var(--accent);
}

/* kaitkan dark theme ke html dataset */
html[data-theme="dark"] body {
  background: var(--bg);
}

/* tooltip elemen yang disematkan via directive v-tooltip */

.cm-tooltip {
  position: fixed;
  top: 0;
  left: 0;
  max-width: 280px;
  background: var(--fg);
  color: var(--bg);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, system-ui, sans-serif;
  font-size: 12px;
  line-height: 1.35;
  padding: 5px 9px;
  border-radius: 7px;
  box-shadow: var(--shadow);
  pointer-events: none;
  opacity: 0;
  transition: opacity 0.12s ease;
  white-space: normal;
  word-break: break-word;
  z-index: 60;
}

.cm-tooltip.visible {
  opacity: 1;
}

.print-flow {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, system-ui, sans-serif;
  line-height: 1.6;
  background: #ffffff !important;
  color: #1c2024 !important;
  word-break: break-word;
  overflow-wrap: break-word;
}

.print-flow h1,
.print-flow h2,
.print-flow h3,
.print-flow h4,
.print-flow h5,
.print-flow h6 {
  color: #111827 !important;
  page-break-after: avoid;
  break-after: avoid;
}

.print-flow p,
.print-flow li,
.print-flow td,
.print-flow th {
  word-break: break-word;
  overflow-wrap: break-word;
}

.print-flow pre,
.print-flow pre.hljs {
  white-space: pre-wrap !important;
  word-break: break-word !important;
  overflow-wrap: break-word !important;
  background: #f6f8fa !important;
  color: #24292f !important;
  border: 1px solid #d0d7de !important;
  border-radius: 6px;
  page-break-inside: avoid;
  break-inside: avoid;
}

.print-flow pre.hljs code {
  white-space: pre-wrap !important;
  word-break: break-word !important;
  overflow-wrap: break-word !important;
}

.print-flow table {
  width: 100% !important;
  table-layout: auto;
  border-collapse: collapse;
  page-break-inside: avoid;
  break-inside: avoid;
}

.print-flow th,
.print-flow td {
  border: 1px solid #d0d7de !important;
}

.print-flow .mermaid-block {
  justify-content: flex-start !important;
}

.print-flow .mermaid-block,
.print-flow .katex-display {
  page-break-inside: avoid;
  break-inside: avoid;
}

.print-flow .mermaid-block svg {
  max-width: 100% !important;
  height: auto !important;
  font-size: 11px !important;
}

.print-flow .mermaid-block text,
.print-flow .mermaid-block .label text,
.print-flow .mermaid-block .node text,
.print-flow .mermaid-block .commit-label,
.print-flow .mermaid-block .branch-label {
  font-size: 11px !important;
}
</style>
