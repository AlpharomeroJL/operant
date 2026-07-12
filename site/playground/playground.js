// Drives the docs-site playground demo: fetches the checked-in compiled
// workflow and fixture page, replays them through the wasm-compiled
// `operant-replay` executor for real verification, and separately narrates
// the same step sequence into the visible iframe so a visitor watches it
// happen rather than reading a pass/fail summary.
import init, { replay_fixture as replayFixture } from "./pkg/operant_replay.js";

const STEP_DELAY_MS = 550;
const STORAGE_KEY = "operant-fixture-invoices";

const button = document.getElementById("replay-btn");
const status = document.getElementById("playground-status");
const log = document.getElementById("playground-log");
const frame = document.getElementById("fixture-frame");

let wasmReady = null;

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function setStatus(text, cls) {
  status.textContent = text;
  status.classList.remove("status-pass", "status-fail");
  if (cls) status.classList.add(cls);
}

function clearLog() {
  log.textContent = "";
}

function logStep(text) {
  const li = document.createElement("li");
  li.textContent = text;
  li.classList.add("step-active");
  log.appendChild(li);
  return li;
}

async function fetchText(path) {
  const res = await fetch(path);
  if (!res.ok) {
    throw new Error(`fetch ${path} failed: ${res.status}`);
  }
  return res.text();
}

function reloadFrame() {
  return new Promise((resolve) => {
    frame.addEventListener("load", resolve, { once: true });
    frame.src = frame.src;
  });
}

function frameDoc() {
  return frame.contentDocument;
}

// Mirror one compiled-workflow action into the visible iframe. Only
// `adapter_call` steps in the `browser` namespace touch the DOM; `wait`
// steps are narration-only pauses, matching what `Replayer::replay`
// actually dispatches (every non-assert action, in order).
async function playAction(action, narration) {
  const li = logStep(narration || action.id);
  await sleep(STEP_DELAY_MS);

  if (action.kind === "adapter_call" && action.params?.namespace === "browser") {
    const verb = action.params.verb;
    const args = action.params.args || {};
    const doc = frameDoc();
    const el = args.selector ? doc.querySelector(args.selector) : null;
    if (verb === "type" && el) {
      el.value = args.text ?? "";
      el.dispatchEvent(new Event("input", { bubbles: true }));
    } else if (verb === "click" && el) {
      el.click();
    }
    // `assert` is already verified inside the wasm call; nothing to mirror
    // visually beyond the narration line itself.
  }

  li.classList.remove("step-active");
}

async function runReplay() {
  button.disabled = true;
  clearLog();
  setStatus("Loading the wasm module...");

  try {
    if (!wasmReady) {
      wasmReady = init();
    }
    await wasmReady;

    const [workflowText, pageHtml] = await Promise.all([
      fetchText("fixtures/compiled_workflow.json"),
      fetchText("fixtures/webapp.html"),
    ]);

    setStatus("Verifying the compiled workflow in wasm...");
    let resultJson;
    try {
      resultJson = replayFixture(workflowText, pageHtml);
    } catch (err) {
      setStatus(`Replay failed: ${err}`, "status-fail");
      button.disabled = false;
      return;
    }
    const result = JSON.parse(resultJson);

    // Reset the visible fixture app to a clean slate, then narrate the
    // same step sequence the wasm call just verified.
    localStorage.removeItem(STORAGE_KEY);
    await reloadFrame();

    const workflow = JSON.parse(workflowText);
    const actions = workflow.actions || [];
    const summaries = workflow.manifest?.step_summary || [];
    setStatus("Replaying...");
    for (let i = 0; i < actions.length; i++) {
      await playAction(actions[i], summaries[i] || actions[i].intent || actions[i].id);
    }

    const passed = result.pre_pass && result.post_pass;
    setStatus(
      `Replay ${passed ? "passed" : "reported a gate failure"}: ` +
        `${result.steps_executed} step(s) executed, preconditions ${
          result.pre_pass ? "passed" : "failed"
        }, postconditions ${result.post_pass ? "passed" : "failed"}.`,
      passed ? "status-pass" : "status-fail",
    );
  } catch (err) {
    setStatus(`Replay failed: ${err}`, "status-fail");
  } finally {
    button.disabled = false;
  }
}

button.addEventListener("click", () => {
  runReplay();
});
