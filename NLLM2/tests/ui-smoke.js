// Headless UI smoke test: loads the real index.html + app.js in jsdom against a
// live Forge server, then drives the actual UI flows a user performs.
const fs = require("fs");
const path = require("path");
const { JSDOM, VirtualConsole } = require("jsdom");

const WEB = "/home/user/testtemp/NLLM2/apps/web";
const BASE = process.env.FORGE_URL || "http://127.0.0.1:8099";

function check(name, cond, detail = "") {
  const ok = !!cond;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${ok || !detail ? "" : "  -> " + detail}`);
  if (!ok) process.exitCode = 1;
}

async function main() {
  const html = fs.readFileSync(path.join(WEB, "index.html"), "utf8");
  const vc = new VirtualConsole();
  const errors = [];
  vc.on("jsdomError", (e) => errors.push(String(e)));
  vc.on("error", (...a) => errors.push(a.join(" ")));

  const dom = new JSDOM(html, {
    url: BASE + "/",
    runScripts: "outside-only",
    pretendToBeVisual: true,
    virtualConsole: vc,
  });
  const { window } = dom;

  // jsdom has no fetch; forward to the live server.
  window.fetch = (url, init) => fetch(BASE + url, init);
  window.confirm = () => true;
  window.prompt = () => null;
  window.scrollTo = () => {};
  Object.defineProperty(window.HTMLElement.prototype, "scrollIntoView", { value() {}, writable: true });

  const appjs = fs.readFileSync(path.join(WEB, "app.js"), "utf8");
  window.eval(appjs);
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const $ = (sel) => window.document.querySelector(sel);
  const $$ = (sel) => [...window.document.querySelectorAll(sel)];
  const byText = (sel, text) => $$(sel).find((e) => e.textContent.trim() === text);

  await wait(600);

  // --- library view
  check("library renders heading", $(".library h1")?.textContent === "Notebooks");
  check("engine badge filled from capabilities", /engine: extractive/.test($("#engineBadge").textContent));
  check("notebook cards render", $$(".card").length >= 1, `cards=${$$(".card").length}`);

  // --- create a notebook through the form
  const form = $(".new-card form");
  form.querySelector('[name="title"]').value = "UI smoke notebook";
  form.dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }));
  await wait(700);
  check("navigated into new notebook", window.location.hash.startsWith("#/nb/"));
  const nbId = window.location.hash.split("/")[2];

  check("three panes mounted", $$(".pane").length === 3);

  // --- import pasted text
  const ta = $("#pasteText");
  check("paste box present", !!ta);
  ta.value =
    "Photosynthesis converts light energy into chemical energy.\n\nThe Calvin cycle fixes carbon dioxide into glucose.\n\nChlorophyll absorbs light most strongly in the blue and red bands.";
  $("#pasteTitle").value = "Biology notes";
  byText(".pane button", "Import text").click();
  await wait(800);
  check("source appears in list", $$(".source-item").length === 1, `sources=${$$(".source-item").length}`);
  check("source shows passage count", /passages/.test($(".source-item .sub")?.textContent || ""));

  // --- ask a grounded question
  const ask = $(".composer textarea");
  ask.value = "What does the Calvin cycle fix?";
  byText(".composer button", "Ask").click();
  await wait(900);
  const answers = $$(".msg.assistant");
  const last = answers[answers.length - 1];
  check("assistant answered", /Calvin cycle|glucose|carbon dioxide/i.test(last.textContent), last.textContent.slice(0, 200));
  const citeBtn = last.querySelector(".cite");
  check("citation chip rendered", !!citeBtn, last.textContent.slice(0, 200));
  check("citation chip has a number", /\[1\]/.test(citeBtn?.textContent || ""));

  // --- open the citation -> source viewer at the evidence
  citeBtn.click();
  await wait(700);
  const viewer = $(".overlay .viewer");
  check("viewer opened", !!viewer, "no overlay");
  check("viewer shows source text", /Calvin cycle/i.test(viewer.textContent));
  check("cited passage is highlighted", !!$(".chunk.hit"));
  byText(".viewer header button", "✕").click();
  await wait(200);
  check("viewer closes on ✕", !$(".overlay"));

  // --- source guide is quoted, labelled, and usable
  const guideSource = $$(".source-item").find((el) => /Biology notes/.test(el.textContent));
  check("source item offers a guide", !!guideSource && [...guideSource.querySelectorAll("button")].some((b) => /guide/i.test(b.textContent)));
  guideSource.querySelector(".name").click();
  await wait(700);
  const guide = $(".overlay .guide");
  check("source guide renders", !!guide, "no .guide block");
  check("guide says it is quoted, not generated", /quoted, not generated/.test(guide?.textContent || ""), guide?.textContent?.slice(0, 160));
  check("guide quotes the source verbatim", /“Photosynthesis converts light energy/.test(guide?.textContent || ""), guide?.textContent?.slice(0, 200));
  check("guide quotes carry a locator", /paragraph \d+/.test(guide?.textContent || ""));
  check("guide lists recurring terms", guide?.querySelectorAll(".chips .badge").length > 0);
  const askBtn = guide?.querySelector(".cites .cite");
  check("guide suggests a question", !!askBtn, "no suggested question");
  if (askBtn) {
    askBtn.click();
    await wait(100);
    check("clicking a suggestion fills the composer", /\?/.test($(".composer textarea")?.value || ""), $(".composer textarea")?.value);
  }
  check("guide links quotes to passages", !!guide?.querySelector(".guide-quote"));
  byText(".viewer header button", "✕").click();
  await wait(200);

  // --- study tools
  byText(".tabs button", "Study").click();
  await wait(100);
  byText("#sideContent button", "Flashcards").click();
  await wait(800);
  const cards = $$(".flashcard");
  check("flashcards generated", cards.length > 0, `cards=${cards.length}`);
  if (cards.length) {
    const inner = cards[0].querySelector(".inner");
    const before = inner.textContent;
    inner.click();
    check("flashcard flips", inner.textContent !== before, `${before} -> ${inner.textContent}`);
  }

  // --- quiz
  byText("#sideContent button", "Quiz").click();
  await wait(700);
  check("quiz generated with options", $$(".quiz-q").length > 0, `questions=${$$(".quiz-q").length}`);
  if ($$(".quiz-q").length) {
    const q = $(".quiz-q");
    const opts = [...q.querySelectorAll(".opt")];
    check("quiz has multiple options", opts.length >= 2, `opts=${opts.length}`);
    opts[0].click();
    await wait(50);
    check("quiz reveals the correct answer", !!q.querySelector(".opt.correct") && opts.every((o) => o.disabled));
  }

  // --- mind map
  byText("#sideContent button", "Concept map").click();
  await wait(700);
  check("mindmap svg rendered", !!$(".mindmap svg circle"), "no svg nodes");

  // --- notes
  byText(".tabs button", "Notes").click();
  await wait(100);
  $("#sideContent textarea").value = "Remember to verify abstention behaviour.";
  byText("#sideContent button", "Save note").click();
  await wait(800);
  check("note saved and listed", /Remember to verify/.test($("#sideContent").textContent));
  check("note offers conversion to a source",
    [...window.document.querySelectorAll("#sideContent button")]
      .some((b) => /use as source/i.test(b.textContent)));

  // --- report
  byText(".tabs button", "Report").click();
  await wait(100);
  byText("#sideContent button", "Build report").click();
  await wait(900);
  check("report built with citations", $$("#sideContent .cite").length > 0);

  // --- a spreadsheet question is answered by computing over real cells
  window.location.hash = `#/nb/${nbId}`;
  await wait(500);
  const sheetPath = path.join(WEB, "../../tests/fixtures/sales.xlsx");
  const sheetFile = new window.File([fs.readFileSync(sheetPath)], "sales.xlsx", {
    type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  });
  const uploadInput = $("#fileInput");
  check("upload control accepts spreadsheets", /\.xlsx/.test(uploadInput?.getAttribute("accept") || ""));
  Object.defineProperty(uploadInput, "files", { value: [sheetFile], writable: true });
  uploadInput.dispatchEvent(new window.Event("change", { bubbles: true }));
  await wait(1500);
  const sheetSources = $$(".source-item");
  const sheetSource = sheetSources.find((el) => /sales\.xlsx/i.test(el.textContent));
  check("spreadsheet imported through the UI", !!sheetSource, sheetSources.map((e) => e.textContent).join(" | "));
  check("source item reports its sheets and rows",
    /2 sheets/.test(sheetSource?.textContent || "") && /6 rows/.test(sheetSource?.textContent || ""),
    sheetSource?.textContent);

  const askSheet = $(".composer textarea");
  askSheet.value = "what is the total revenue";
  byText(".composer button", "Ask").click();
  await wait(1200);
  const sheetAnswers = $$(".msg.assistant");
  const computedMsg = sheetAnswers[sheetAnswers.length - 1];
  check("spreadsheet total is computed, not quoted", /2350\.5/.test(computedMsg.textContent), computedMsg.textContent.slice(0, 300));
  check("computed answer says so", /computed from table cells/.test(computedMsg.textContent), computedMsg.textContent.slice(0, 200));
  check("computed answer names the excluded subtotal row", /C4/.test(computedMsg.textContent), computedMsg.textContent.slice(0, 300));
  const sheetCite = computedMsg.querySelector(".cite");
  check("computed answer carries a cell citation", /Sales/.test(sheetCite?.textContent || ""), sheetCite?.textContent);

  sheetCite.click();
  await wait(800);
  const gridViewer = $(".overlay .viewer");
  check("viewer renders the real grid", !!gridViewer?.querySelector("table.grid"), "no grid");
  check("grid keeps original row numbers",
    gridViewer?.querySelector("tbody tr th.rownum")?.textContent === "2",
    gridViewer?.querySelector("tbody tr")?.textContent);
  check("grid shows the cached formula value", /2050\.5/.test(gridViewer?.textContent || ""));
  check("grid cells expose their A1 reference", !!gridViewer?.querySelector('td[title^="C2"]'));
  const hiddenNote = /hidden sheet/.test(gridViewer?.textContent || "");
  check("hidden sheets are labelled when shown", hiddenNote, "Meta sheet not labelled");
  byText(".viewer header button", "✕").click();
  await wait(200);

  // --- URL import is a real control, not a prompt() box
  check("add-from-URL control exists", !!window.document.querySelector("#addUrlBtn"));
  check("URL input is labelled for assistive tech",
    !!window.document.querySelector('input[aria-label="Web page URL"]'));

  // --- capabilities page tells the truth
  window.location.hash = "#/capabilities";
  await wait(600);
  const capsText = $(".library")?.textContent || "";
  check("capabilities lists supported pdf", /pdf/i.test(capsText));
  check("capabilities lists audio as unsupported", /audio/i.test(capsText) && /unsupported/i.test(capsText));
  check("capabilities discloses the NotebookLM bridge state", /target unreachable/i.test(capsText));
  check("bridge panel exposes inspectable RPCs", /RPC methods recovered/.test(capsText));

  // --- binary inspection panel (real report rendered through the real renderer)
  check("capabilities reports URL ingestion with its transport",
    /web_fetch/i.test(capsText) && /curl/i.test(capsText));
  check("informational capability rows are not shown as feature claims",
    !/\binfo\b.*configured/i.test(capsText));
  check("capabilities states the binary-analysis scope",
    /binary_analysis/.test(capsText) && /apk java decompilation/i.test(capsText) && /disassembly/i.test(capsText));
  const fileInput = window.document.querySelector('input[type="file"]');
  check("binary panel exposes a file picker", !!fileInput && /\.apk/.test(fileInput.getAttribute("accept") || ""));

  const apkPath = path.join(WEB, "../../tests/fixtures/apk/sample.apk");
  const apkRes = await fetch(`${BASE}/api/binary/inspect?name=sample.apk`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: fs.readFileSync(apkPath),
  });
  const apkReport = (await apkRes.json()).report;
  const rendered = window.renderBinaryReport(apkReport);
  const host = window.document.createElement("div");
  host.appendChild(rendered);
  const apkText = host.textContent;
  check("binary report renders the package identity", /com\.example\.forgetest/.test(apkText));
  check("binary report renders DEX code findings", /crypto/i.test(apkText) && /cleartext/i.test(apkText));
  check("binary report renders risk indicators", /debuggable/i.test(apkText));

  const exeRes = await fetch(`${BASE}/api/binary/inspect?name=sample.exe`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: fs.readFileSync(path.join(WEB, "../../tests/fixtures/sample.exe")),
  });
  const exeHost = window.document.createElement("div");
  exeHost.appendChild(window.renderBinaryReport((await exeRes.json()).report));
  const exeText = exeHost.textContent;
  check("binary report renders PE imports", /KERNEL32\.dll/.test(exeText));
  check("binary report renders packer signals", /W\+X|high-entropy/i.test(exeText));

  // --- mobile layout: the CSS must define breakpoints
  const css = fs.readFileSync(path.join(WEB, "styles.css"), "utf8");
  check("responsive breakpoints present", /@media \(max-width: 620px\)/.test(css) && /@media \(max-width: 980px\)/.test(css));

  check("no uncaught page errors", errors.length === 0, errors.slice(0, 3).join(" | "));

  // cleanup
  await fetch(`${BASE}/api/notebooks/${nbId}`, { method: "DELETE" });
  dom.window.close();
  console.log("\nUI smoke finished");
}

main().catch((e) => {
  console.error("FATAL", e);
  process.exit(1);
});
