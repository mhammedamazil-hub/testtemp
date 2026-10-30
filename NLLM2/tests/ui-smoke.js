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

  // --- report
  byText(".tabs button", "Report").click();
  await wait(100);
  byText("#sideContent button", "Build report").click();
  await wait(900);
  check("report built with citations", $$("#sideContent .cite").length > 0);

  // --- capabilities page tells the truth
  window.location.hash = "#/capabilities";
  await wait(600);
  const capsText = $(".library")?.textContent || "";
  check("capabilities lists supported pdf", /pdf/i.test(capsText));
  check("capabilities lists audio as unsupported", /audio/i.test(capsText) && /unsupported/i.test(capsText));

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
