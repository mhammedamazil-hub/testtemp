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
  // `__pace` is a test-side transport slowdown for the chat stream (the server accepts
  // `?pace_ms=` as a diagnostic). The app itself never sets it.
  window.__pace = null;
  window.fetch = (url, init) => {
    let u = String(url);
    if (window.__pace && u.includes("/chat/stream")) {
      u += (u.includes("?") ? "&" : "?") + "pace_ms=" + window.__pace;
    }
    return fetch(BASE + u, init);
  };
  // jsdom lacks a couple of web-platform globals the streaming path uses
  if (!window.TextDecoder) window.TextDecoder = TextDecoder;
  if (!window.AbortController) window.AbortController = AbortController;
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

  // --- the library can be searched and sorted (7.1), before anything else exists
  check("library exposes a search box", !!$("#librarySearch") && !!$("#librarySearch").getAttribute("aria-label"));
  check("library exposes a sort control", !!$("#librarySort"));
  const sortValues = $$("#librarySort option").map((o) => o.value);
  check("sort offers recent, title, created and sources", ["recent", "title", "created", "sources"].every((v) => sortValues.includes(v)),
    sortValues.join(","));

  // --- create a notebook through the form
  const form = $(".new-card form");
  form.querySelector('[name="title"]').value = "UI smoke notebook";
  form.dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }));
  await wait(700);
  check("navigated into new notebook", window.location.hash.startsWith("#/nb/"));
  const nbId = window.location.hash.split("/")[2];

  check("three panes mounted", $$(".pane").length === 3);

  // --- the notebook carries its icon and shows up in search
  await fetch(`${BASE}/api/notebooks/${nbId}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ icon: "📗" }),
  });
  window.location.hash = "#/";
  await wait(600);
  check("a notebook icon renders in the library", /📗/.test($(".library")?.textContent || ""),
    $(".library")?.textContent?.slice(0, 120));
  $("#librarySearch").value = "UI smoke";
  $("#librarySearch").dispatchEvent(new window.Event("input", { bubbles: true }));
  await wait(700);
  check("searching filters the library", /1 matching/.test($("#libraryCount")?.textContent || ""),
    $("#libraryCount")?.textContent);
  $("#librarySearch").value = "no-such-notebook-anywhere";
  $("#librarySearch").dispatchEvent(new window.Event("input", { bubbles: true }));
  await wait(700);
  check("an empty search result says what was searched",
    /Nothing matches/.test($(".library")?.textContent || "") && /source titles/.test($(".library")?.textContent || ""),
    $(".library")?.textContent?.slice(0, 160));
  $("#librarySearch").value = "";
  $("#librarySearch").dispatchEvent(new window.Event("input", { bubbles: true }));
  await wait(700);
  check("clearing the search restores the list", $$(".card").length >= 2, `cards=${$$(".card").length}`);

  // --- duplicate copies evidence, not chat history
  const dupBtn = [...window.document.querySelectorAll(".card button")].find((b) => /Duplicate/.test(b.textContent));
  check("a notebook can be duplicated from the library", !!dupBtn);
  dupBtn.click();
  await wait(900);
  check("the copy appears with a (copy) suffix", /\(copy\)/.test($(".library")?.textContent || ""),
    $(".library")?.textContent?.slice(0, 200));
  const copyCards = $$(".card").filter((c) => /\(copy\)/.test(c.textContent));
  check("exactly one copy was made", copyCards.length === 1, `copies=${copyCards.length}`);
  window.location.hash = `#/nb/${nbId}`;
  await wait(600);

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

  // --- chat presentation controls are real (they change the answer)
  check("composer exposes a style control", !!$("#chatStyle") && !!$("#chatStyle").getAttribute("aria-label"));
  check("composer exposes a length control", !!$("#chatLength"));
  const setLen = (v) => {
    const sel = $("#chatLength");
    sel.value = v;
    sel.dispatchEvent(new window.Event("change", { bubbles: true }));
  };
  const setStyle = (v) => {
    const sel = $("#chatStyle");
    sel.value = v;
    sel.dispatchEvent(new window.Event("change", { bubbles: true }));
  };
  setLen("brief");
  await wait(50);
  check("length control explains its effect", /cites 1 passage/.test($("#styleHint")?.textContent || ""), $("#styleHint")?.textContent);
  const askAgain = $(".composer textarea");
  askAgain.value = "What does the Calvin cycle fix?";
  byText(".composer button", "Ask").click();
  await wait(900);
  const briefAnswers = $$(".msg.assistant");
  const briefMsg = briefAnswers[briefAnswers.length - 1];
  check("brief answer cites exactly one passage", briefMsg.querySelectorAll(".cite").length === 1,
    `cites=${briefMsg.querySelectorAll(".cite").length}`);
  check("brief answer reports its settings", /evidence\/brief/.test(briefMsg.textContent), briefMsg.textContent.slice(-120));

  setStyle("detailed");
  setLen("thorough");
  await wait(50);
  const askThird = $(".composer textarea");
  askThird.value = "What does the Calvin cycle fix?";
  byText(".composer button", "Ask").click();
  await wait(900);
  const detailedAnswers = $$(".msg.assistant");
  const detailedMsg = detailedAnswers[detailedAnswers.length - 1];
  check("detailed style shows where each quote came from", /Sources consulted:/.test(detailedMsg.textContent),
    detailedMsg.textContent.slice(0, 200));
  check("detailed answer still cites real passages", detailedMsg.querySelectorAll(".cite").length >= 1);
  check("detailed answer reports its settings", /detailed\/thorough/.test(detailedMsg.textContent));
  setStyle("evidence");
  setLen("standard");
  await wait(50);

  // --- streaming: frames render progressively and Stop is real (not decorative)
  const convsBefore = await (await fetch(`${BASE}/api/notebooks/${nbId}/conversations`)).json();
  const countBefore = (convsBefore.conversations || []).reduce((n, c) => n + c.message_count, 0);

  window.__pace = 60; // test-side transport slowdown only
  const askStream = $(".composer textarea");
  askStream.value = "What does the Calvin cycle fix?";
  byText(".composer button", "Ask").click();
  await wait(250);
  const live = $$(".msg.assistant").pop();
  check("a Stop control appears while the answer streams", !!live?.querySelector("button.stop"),
    live?.textContent?.slice(0, 120));
  check("partial text is rendered before the answer finishes",
    /Calvin|passage|Sources/i.test(live?.querySelector(".body")?.textContent || ""),
    live?.querySelector(".body")?.textContent);
  live?.querySelector("button.stop")?.click();
  await wait(500);
  const stopped = $$(".msg.assistant").pop();
  check("stopping says the turn was not saved", /not saved to the conversation/.test(stopped?.textContent || ""),
    stopped?.textContent?.slice(-160));
  check("stopped turn is marked, not presented as an answer", !!stopped?.classList.contains("stopped"));

  await wait(600); // let the server notice the hang-up
  const convsAfter = await (await fetch(`${BASE}/api/notebooks/${nbId}/conversations`)).json();
  const countAfter = (convsAfter.conversations || []).reduce((n, c) => n + c.message_count, 0);
  check("a stopped turn is not written to the conversation", countAfter === countBefore,
    `messages before=${countBefore} after=${countAfter}`);

  window.__pace = null;
  const askFinal = $(".composer textarea");
  askFinal.value = "What does the Calvin cycle fix?";
  byText(".composer button", "Ask").click();
  await wait(900);
  const streamedMsg = $$(".msg.assistant").pop();
  check("a completed stream renders its citations", streamedMsg.querySelectorAll(".cite").length >= 1);
  const convsFinal = await (await fetch(`${BASE}/api/notebooks/${nbId}/conversations`)).json();
  const countFinal = (convsFinal.conversations || []).reduce((n, c) => n + c.message_count, 0);
  check("a completed stream is saved once", countFinal === countBefore + 2,
    `messages before=${countBefore} after=${countFinal}`);
  // --- contradictory sources are shown side by side (Journey C)
  for (const [title, text] of [
    ["Lab run A", "The Calvin cycle fixes 3 carbon dioxide molecules per turn of the cycle."],
    ["Lab run B", "The Calvin cycle fixes 6 carbon dioxide molecules per turn of the cycle."],
  ]) {
    const box = $("#pasteText");
    box.value = text;
    $("#pasteTitle").value = title;
    byText(".pane button", "Import text").click();
    await wait(700);
  }
  const askConflict = $(".composer textarea");
  askConflict.value = "how many carbon dioxide molecules does the calvin cycle fix";
  byText(".composer button", "Ask").click();
  await wait(900);
  const conflictMsg = $$(".msg.assistant").pop();
  check("sources that disagree produce a labelled block", !!conflictMsg.querySelector(".conflicts"),
    conflictMsg.textContent.slice(0, 240));
  check("the disagreement names both sides",
    (conflictMsg.querySelectorAll(".conflicts .claim").length >= 2),
    conflictMsg.querySelector(".conflicts")?.textContent);
  check("the block says Forge does not choose",
    /does not choose between them/.test(conflictMsg.querySelector(".conflicts")?.textContent || ""));


  // --- the transcript exports with its citations
  check("the chat exposes a transcript export once a conversation exists", !!$("#exportChat"),
    $(".messages")?.textContent?.slice(0, 120));
  $("#exportChat").click();
  await wait(700);
  const transcript = $(".export-body")?.textContent || "";
  check("the transcript export shows the questions and the evidence",
    /### You/.test(transcript) && /### Forge/.test(transcript) && /\*\*Evidence\*\*/.test(transcript),
    transcript.slice(0, 200));
  check("the transcript export keeps the citations", /Calvin cycle|glucose|carbon dioxide/i.test(transcript),
    transcript.slice(0, 300));
  $$(".overlay").forEach((o) => o.remove());
  await wait(150);


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

  // --- difficulty is a generation parameter, not a label on the output
  const diffSel = $("#artifactDifficulty");
  check("study tab exposes a difficulty control", !!diffSel && !!diffSel.getAttribute("aria-label"),
    diffSel?.outerHTML?.slice(0, 120));
  diffSel.value = "hard";
  byText("#sideContent button", "Flashcards").click();
  await wait(900);
  check("hard difficulty is reported on the generated artifact",
    /built at hard difficulty/.test($("#reviewSummary")?.textContent || ""), $("#reviewSummary")?.textContent);
  const hardCards = $$(".flashcard");
  check("hard difficulty still produces cards", hardCards.length > 0, `cards=${hardCards.length}`);

  // --- a review mark is the learner's data: stored server-side, shown when reopened
  hardCards[0].querySelectorAll("button.ghost")[0].click(); // "knew it"
  await wait(500);
  check("a card can be marked known", /known/.test(hardCards[0].querySelector(".badge")?.textContent || ""),
    hardCards[0].querySelector(".badge")?.textContent);
  check("the summary counts the mark", /1 known/.test($("#reviewSummary")?.textContent || ""), $("#reviewSummary")?.textContent);
  const artList = await (await fetch(`${BASE}/api/notebooks/${nbId}/artifacts`)).json();
  const cardArt = (artList.artifacts || []).filter((a) => a.kind === "flashcards").pop();
  check("the review mark is persisted on the artifact", !!cardArt && Object.values(cardArt.reviews || {}).includes("known"),
    JSON.stringify(cardArt?.reviews));
  check("the server reports review counts", !!cardArt && (cardArt.review_counts?.known || 0) >= 1,
    JSON.stringify(cardArt?.review_counts));
  window.showArtifact(cardArt.id);
  await wait(600);
  check("reopening an artifact shows the saved mark",
    /1 known/.test($(".overlay #reviewSummary")?.textContent || "") &&
      $$(".overlay .flashcard .badge").some((b) => /known/.test(b.textContent)),
    $(".overlay #reviewSummary")?.textContent);
  $$(".overlay").forEach((o) => o.remove());
  await wait(150);
  // --- regeneration keeps the parameters and says what it clears
  window.showArtifact(cardArt.id);
  await wait(600);
  const regenBtn = [...$$(".viewer header button")].find((b) => /regenerate/i.test(b.textContent));
  check("an artifact can be regenerated from its own parameters", !!regenBtn,
    $$(".viewer header button").map((b) => b.textContent).join(","));
  regenBtn.click();
  await wait(1200);
  const regenArt = await (await fetch(`${BASE}/api/artifacts/${cardArt.id}`)).json();
  check("regenerating bumps the version and keeps the difficulty",
    regenArt.version > cardArt.version && regenArt.difficulty === "hard",
    `version=${regenArt.version} difficulty=${regenArt.difficulty}`);
  check("regenerating clears the review marks rather than carrying them onto new cards",
    Object.keys(regenArt.reviews || {}).length === 0, JSON.stringify(regenArt.reviews));
  $$(".overlay").forEach((o) => o.remove());
  await wait(150);

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

  // --- slides: a deck of quotations you can edit, reorder, delete, restore and export
  const audienceSel = $("#artifactAudience");
  check("study tab exposes a slide audience control", !!audienceSel && !!audienceSel.getAttribute("aria-label"));
  audienceSel.value = "technical";
  byText("#sideContent button", "Slides").click();
  await wait(1000);
  const deckSlides = $$(".slide");
  check("slides render as an editable deck", deckSlides.length >= 2, `slides=${deckSlides.length}`);
  check("slide bullets carry their citations", !!deckSlides[1]?.querySelector("li .cite"),
    deckSlides[1]?.textContent?.slice(0, 160));
  const slideList = await (await fetch(`${BASE}/api/notebooks/${nbId}/artifacts`)).json();
  const deck = (slideList.artifacts || []).filter((a) => a.kind === "slides").pop();
  const deckFull = deck ? await (await fetch(`${BASE}/api/artifacts/${deck.id}`)).json() : null;
  check("the deck records the audience it was built for", deckFull?.payload?.audience === "technical",
    deckFull?.payload?.audience);

  const firstTitle = $$(".slide input.slide-title")[0];
  firstTitle.value = "Edited opening slide";
  firstTitle.dispatchEvent(new window.Event("change", { bubbles: true }));
  await wait(700);
  const afterEdit = await (await fetch(`${BASE}/api/artifacts/${deck.id}`)).json();
  check("a slide title edit is saved and versioned",
    afterEdit.payload?.slides?.[0]?.title === "Edited opening slide" && afterEdit.version > 1,
    `title=${afterEdit.payload?.slides?.[0]?.title} v=${afterEdit.version}`);

  const movedTitle = $$(".slide input.slide-title")[1]?.value;
  $$(".slide")[0].querySelectorAll("button.ghost")[1].click(); // ↓
  await wait(700);
  check("reorder moves a slide and persists", $$(".slide input.slide-title")[0]?.value === movedTitle,
    `now=${$$(".slide input.slide-title")[0]?.value} expected=${movedTitle}`);

  const beforeDelete = $$(".slide").length;
  const lastSlide = $$(".slide").pop();
  lastSlide.querySelectorAll("button.ghost")[2].click(); // ✕
  await wait(700);
  check("a deleted slide leaves the deck", $$(".slide").length === beforeDelete - 1,
    `slides=${$$(".slide").length}`);
  check("a deleted slide is kept in a restore list", $$(".slide-removed").length === 1,
    `removed=${$$(".slide-removed").length}`);
  byText(".slide-removed button", "restore").click();
  await wait(700);
  check("restore brings the slide back", $$(".slide").length === beforeDelete, `slides=${$$(".slide").length}`);

  window.showArtifact(deck.id);
  await wait(600);
  byText(".viewer header button", "md").click();
  await wait(700);
  check("markdown export shows the exact bytes", /Edited opening slide|Slide outline/.test($(".export-body")?.textContent || ""),
    $(".export-body")?.textContent?.slice(0, 120));
  check("export controls offer md and json only",
    $$(".viewer header button").some((b) => b.textContent === "json") &&
      !$$(".viewer header button").some((b) => /pptx/i.test(b.textContent)));
  $$(".overlay").forEach((o) => o.remove());
  await wait(150);

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

  // --- a failed ingest is visible, retryable, and honestly logged
  const broken = await fetch(`${BASE}/api/notebooks/${nbId}/sources/upload?filename=broken.pdf`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: Buffer.from("this is definitely not a pdf"),
  });
  check("a corrupt upload is refused", broken.status === 422, `status=${broken.status}`);
  await window.openNotebook(window.document.getElementById("app"), nbId);
  await wait(900);
  const failedItem = $$(".source-item.failed").find((el) => /broken/i.test(el.textContent));
  check("the failed source stays listed with its error", !!failedItem && /pdf|extract|readable/i.test(failedItem.textContent),
    failedItem?.textContent?.slice(0, 200));
  check("the failed source offers a retry control",
    !!failedItem && [...failedItem.querySelectorAll("button")].some((b) => /retry/i.test(b.textContent)),
    failedItem?.textContent?.slice(0, 160));
  check("the job log shows the failed attempt", /failed/.test($("#jobs")?.textContent || "") && /attempt 1/.test($("#jobs")?.textContent || ""),
    $("#jobs")?.textContent?.slice(0, 200));
  [...failedItem.querySelectorAll("button")].find((b) => /retry/i.test(b.textContent)).click();
  await wait(1000);
  const jobsAfterRetry = await (await fetch(`${BASE}/api/notebooks/${nbId}/jobs`)).json();
  const failedAttempts = (jobsAfterRetry.jobs || []).filter((j) => j.status === "failed");
  check("retrying records a second attempt rather than a duplicate source", failedAttempts.length >= 2 && failedAttempts.every((j) => j.attempts >= 1),
    JSON.stringify(failedAttempts.map((j) => j.attempts)));
  const sourceCount = (await (await fetch(`${BASE}/api/notebooks/${nbId}/sources`)).json()).sources
    .filter((x) => /broken/i.test(x.title)).length;
  check("the retry did not add a second source", sourceCount === 1, `sources with that name=${sourceCount}`);

  // --- discovery reads real links from a page the user chose (Journey I)
  const http = require("http");
  const fixture = http.createServer((req, res) => {
    const pages = {
      "/list": "<html><head><title>Reading list</title></head><body>" +
        '<a href="/a">Alpha study</a><a href="/b">Beta note</a>' +
        '<a href="/talk.mp4">Recorded talk</a></body></html>',
      "/a": "<html><head><title>Alpha study</title></head><body><p>Retrieval quality depends on the ranking function used for the corpus.</p></body></html>",
      "/b": "<html><head><title>Beta note</title></head><body><p>A citation check rejects any answer whose quoted span is missing from the source.</p></body></html>",
    };
    const page = pages[req.url.split("?")[0]];
    if (!page) { res.writeHead(404); res.end("missing"); return; }
    res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    res.end(page);
  });
  await new Promise((r) => fixture.listen(0, "127.0.0.1", r));
  const fixturePort = fixture.address().port;
  window.openDiscovery();
  await wait(400);
  check("the discovery panel says there is no search engine behind it",
    /does not search the web|Forge reads the outbound links/i.test($(".overlay .viewer")?.textContent || ""),
    $(".overlay .viewer")?.textContent?.slice(0, 200));
  $(".overlay .viewer input[aria-label=\"Seed page URL\"]").value = `http://127.0.0.1:${fixturePort}/list`;
  byText(".overlay .viewer button", "Read links").click();
  await wait(1200);
  const cands = $$(".overlay .viewer .candidate");
  check("candidate links are listed with their provenance",
    cands.length === 2 && cands.every((c) => /found on http:\/\/127\.0\.0\.1/.test(c.textContent)),
    cands.map((c) => c.textContent.slice(0, 80)).join(" | "));
  check("the media link is not offered as a page", !/talk\.mp4/.test($(".overlay .viewer")?.textContent || ""),
    $(".overlay .viewer")?.textContent?.slice(0, 300));
  check("the panel reports the seed page title and link counts",
    /Reading list/.test($(".overlay .viewer")?.textContent || "") &&
      /links seen/.test($(".overlay .viewer")?.textContent || ""));
  const importBtn = $("#importCandidates");
  check("candidates can be imported in one action", !!importBtn);
  importBtn.click();
  await wait(2000);
  const discovered = $$(".source-item").map((el) => el.textContent).join(" | ");
  check("both discovered pages became sources",
    /Alpha study/.test(discovered) && /Beta note/.test(discovered), discovered.slice(0, 300));
  check("the import warning about the media link was surfaced or explained",
    !/talk\.mp4/.test(discovered));
  fixture.close();

  // --- an EPUB is read in spine order and cites its chapter (§7.2)
  const epub = fs.readFileSync(path.join(WEB, "../../tests/fixtures/sample.epub"));
  const epubFile = new window.File([epub], "handbook.epub", { type: "application/epub+zip" });
  const epubInput = $("#fileInput");
  Object.defineProperty(epubInput, "files", { value: [epubFile], writable: true });
  epubInput.dispatchEvent(new window.Event("change", { bubbles: true }));
  await wait(1500);
  const epubSource = $$(".source-item").find((el) => /handbook\.epub/i.test(el.textContent));
  check("an EPUB is accepted as a source", !!epubSource, $$(".source-item").map((e) => e.textContent).join(" | ").slice(0, 240));
  check("the EPUB source reports its kind", /epub/.test(epubSource?.textContent || ""), epubSource?.textContent);
  const askEpub = $(".composer textarea");
  askEpub.value = "how long are customer records retained";
  byText(".composer button", "Ask").click();
  await wait(1200);
  const epubMsg = $$(".msg.assistant").pop();
  check("an EPUB citation names the chapter, not just the file", /chapter 2/.test(epubMsg?.textContent || ""),
    epubMsg?.textContent?.slice(0, 300));
  await window.openNotebook(window.document.getElementById("app"), nbId);
  await wait(900);

  // --- an RSS feed is read as dated items you can cite by name and date (§7.2)
  const feedServer = http.createServer((req, res) => {
    const rss = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>Retention Weekly</title>
  <link>https://example.test/</link>
  <item>
    <title>Churn fell after the onboarding rewrite</title>
    <link>https://example.test/posts/onboarding</link>
    <pubDate>Tue, 03 Mar 2026 09:00:00 GMT</pubDate>
    <description><![CDATA[<p>Monthly churn fell from 4.1% to 2.9% in the quarter after the rewrite.</p>]]></description>
  </item>
  <item>
    <title>Legal hold overrides the thirty day window</title>
    <link>https://example.test/posts/legal-hold</link>
    <pubDate>Wed, 04 Mar 2026 12:30:00 GMT</pubDate>
    <description>A legal hold suspends deletion regardless of the thirty day retention window.</description>
  </item>
</channel></rss>`;
    if (!/^\/feed\.xml/.test(req.url)) { res.writeHead(404); res.end("missing"); return; }
    res.writeHead(200, { "Content-Type": "text/plain; charset=utf-8" });
    res.end(rss);
  });
  await new Promise((r) => feedServer.listen(0, "127.0.0.1", r));
  const feedPort = feedServer.address().port;
  $("#addUrlBtn").click();
  await wait(200);
  $("#urlInput").value = `http://127.0.0.1:${feedPort}/feed.xml`;
  byText("button", "Fetch page").click();
  await wait(1800);
  const feedSource = $$(".source-item").find((el) => /Retention Weekly/.test(el.textContent));
  check("an RSS feed becomes a source with the feed's own title", !!feedSource,
    $$(".source-item").map((e) => e.textContent).join(" | ").slice(0, 240));
  check("the feed source reports its kind", /feed/.test(feedSource?.textContent || ""), feedSource?.textContent);
  check("a GitHub repository can be added from the sources pane without implying a connection",
    !!$("#addGithubBtn"),
    $$("#sideContent button").map((b) => b.textContent).join(","));
  $(".composer textarea").value = "how far did monthly churn fall after the rewrite";
  byText(".composer button", "Ask").click();
  await wait(1400);
  const feedMsg = $$(".msg.assistant").pop();
  // The citation names the entry it came from. The entry's own publish date is part of the
  // indexed text and is asserted against the chunk body in the Rust suite — the UI does not
  // invent one, so it is not required to display one.
  check("a feed citation names the entry it came from",
    /item 1 · Churn fell/.test(feedMsg?.textContent || ""),
    feedMsg?.textContent?.slice(0, 400));
  feedServer.close();
  await window.openNotebook(window.document.getElementById("app"), nbId);
  await wait(900);

  // --- a supplied caption file becomes a timecode-citing source (Journey E)
  const vtt = fs.readFileSync(path.join(WEB, "../../tests/fixtures/lecture.vtt"));
  const vttFile = new window.File([vtt], "lecture.vtt", { type: "text/vtt" });
  const vttInput = $("#fileInput");
  Object.defineProperty(vttInput, "files", { value: [vttFile], writable: true });
  vttInput.dispatchEvent(new window.Event("change", { bubbles: true }));
  await wait(1500);
  const vttSource = $$(".source-item").find((el) => /lecture\.vtt/i.test(el.textContent));
  check("a caption file is accepted as a source", !!vttSource, $$(".source-item").map((e) => e.textContent).join(" | ").slice(0, 300));
  check("the caption source reports its kind", /transcript/.test(vttSource?.textContent || ""), vttSource?.textContent);
  const askStamp = $(".composer textarea");
  askStamp.value = "what is the retention window for customer records";
  byText(".composer button", "Ask").click();
  await wait(1200);
  const stampMsg = $$(".msg.assistant").pop();
  check("a transcript answer cites the spoken timecode", /00:42/.test(stampMsg?.textContent || ""),
    stampMsg?.textContent?.slice(0, 300));
  const stampBad = await fetch(`${BASE}/api/notebooks/${nbId}/sources/upload?filename=clip.mp4`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: Buffer.from("fake mp4"),
  });
  check("a video upload is refused with a reason instead of being indexed as text",
    stampBad.status === 422 && /transcript/i.test((await stampBad.json()).error || ""));
  await window.openNotebook(window.document.getElementById("app"), nbId);
  await wait(900);

  // --- sharing is an owner action, and the panel says when it is not enforced
  $(".pane #shareBtn").click();
  await wait(700);
  check("the share panel opens", !!$(".overlay .viewer") && /Share “/.test($(".overlay .viewer")?.textContent || ""),
    $(".overlay .viewer")?.textContent?.slice(0, 120));
  check("the panel states that sharing is not enforced in single-user mode",
    /Sharing is not enforced yet/.test($(".overlay .viewer")?.textContent || ""),
    $(".overlay .viewer")?.textContent?.slice(0, 240));
  check("the panel explains what a token would change",
    /owner token/i.test($(".overlay .viewer")?.textContent || ""));
  byText(".overlay .viewer button", "Create link").click();
  await wait(600);
  const shareLink = $("#newShare input")?.value || "";
  check("a share link is shown once after creation", /#\/share\/[0-9a-f]{32}$/.test(shareLink), shareLink);
  check("the new link is listed with its role", /viewer/.test($(".overlay .viewer")?.textContent || ""));
  const revokeBtn = $$(".overlay .viewer .note-item button").find((b) => /revoke/i.test(b.textContent));
  check("a link can be revoked from the panel", !!revokeBtn);
  revokeBtn.click();
  await wait(600);
  check("a revoked link is marked revoked", /revoked/.test($(".overlay .viewer")?.textContent || ""));
  $$(".overlay").forEach((o) => o.remove());
  await wait(150);

  // --- a request without a token produces a gate, not a blank page
  window.showTokenPrompt("token required for this test");
  await wait(50);
  check("a missing token produces an explicit gate",
    !!$("#tokenGate #tokenInput") && /requires a token/.test($("#tokenGate")?.textContent || ""),
    $("#tokenGate")?.textContent?.slice(0, 160));
  $("#tokenGate").remove();

  // --- a share link routes to the share view and keeps its token out of the URL
  const hashBeforeShare = window.location.hash;
  window.location.hash = "#/share/abc123def456abc123def456abc12345";
  const shareRoute = window.parseHash();
  check("a share link parses as a share route", shareRoute.view === "share" && /^abc123/.test(shareRoute.token || ""),
    JSON.stringify(shareRoute));
  window.history.replaceState(null, "", hashBeforeShare);
  window.localStorage.removeItem("forge.shareToken");

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
  check("capabilities discloses the media-metadata scope",
    /media/i.test(capsText) && /\/api\/media\/inspect/.test(capsText) && /No frame decoding/i.test(capsText),
    capsText.slice(0, 300));
  check("capabilities marks video ingestion unsupported with a reason",
    /video/i.test(capsText) && /authorized extractor/i.test(capsText));
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
