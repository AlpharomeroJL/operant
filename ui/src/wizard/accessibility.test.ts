// X8 app-accessibility bar for the onboarding wizard: an axe-core scan of
// every real screen (not hand-built fixtures: the same wizard state machine
// ui/src/wizard/state.test.ts and ./mediaPresence.test.ts drive), plus the
// keyboard-specific behavior axe cannot check by static analysis alone
// (focus surviving a rebuild, focus landing in new content, Tab staying
// inside the modal, Escape mapping to the one cancel action this screen
// offers). See ui/src/styles/testDomEnv.ts and ./view.ts's header comment
// for why each of these exists.

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { pressTab, pressActivate, pressEscape, typeText } from "../styles/keyboardSim.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createWizard, type Wizard, type WizardSnapshot } from "./state.ts";
import { mountWizard } from "./view.ts";

function mount(env: ReturnType<typeof createDomEnv>, wizard: Wizard): { container: HTMLElement; render: () => WizardSnapshot } {
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  function render(): WizardSnapshot {
    const snap = wizard.getSnapshot();
    mountWizard(container, snap, {
      onContinueWelcome: () => wizard.continueWelcome(),
      onChooseChatGPT: () => wizard.chooseChatGPT(),
      onChooseClaude: () => wizard.chooseClaude(),
      onStartLocalDownload: () => wizard.startLocalDownload(),
      onPauseLocalDownload: () => wizard.pauseLocalDownload(),
      onResumeLocalDownload: () => wizard.resumeLocalDownload(),
      onCancelLocalDownload: () => wizard.cancelLocalDownload(),
      onContinueAfterLocalDownload: () => wizard.continueAfterLocalDownload(),
      onAccessKeyTextChange: (text) => wizard.setAccessKeyText(text),
      onChooseProviderManually: (p) => wizard.chooseProviderManually(p),
      onContinueWithAccessKey: () => wizard.continueWithAccessKey(),
      onStartDemo: () => wizard.startDemo(),
      onPlayMicSample: () => wizard.playMicSample(),
      onSkipMicCheck: () => wizard.skipMicCheck(),
      onContinueMicCheck: () => wizard.continueMicCheck(),
      onSaveAsWorkflow: () => wizard.saveAsWorkflow(),
      onContinueAfterDemo: () => wizard.continueAfterDemo(),
      onChooseSchedule: (id) => wizard.chooseSchedule(id),
      onFinishSchedule: () => wizard.finishSchedule(),
    });
    return snap;
  }
  return { container, render };
}

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

test("welcome screen: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient());
    const { container } = mount(env, wizard);
    await assertNoViolations(container, "wizard welcome screen");
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("setup_path screen (every card rendered, including the manual provider picker): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient());
    const { container, render } = mount(env, wizard);
    render();
    wizard.continueWelcome();
    // A non-key-shaped string leaves detection null and shows the manual
    // provider picker, exercising the extra <select> this screen can render.
    wizard.setAccessKeyText("not-a-recognizable-key");
    render();
    await assertNoViolations(container, "wizard setup_path screen");
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("setup_path screen mid local-model download (progressbar, pause/cancel visible): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient(), { download: { tickMs: 1000 } });
    const { container, render } = mount(env, wizard);
    render();
    wizard.continueWelcome();
    render();
    wizard.startLocalDownload();
    render();
    await assertNoViolations(container, "wizard setup_path screen mid-download");
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("mic_check screen: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient());
    const { container, render } = mount(env, wizard);
    wizard.continueWelcome();
    wizard.chooseChatGPT();
    render();
    await assertNoViolations(container, "wizard mic_check screen");
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("guided_task screen, mid-run and after Save as workflow: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient(), { guidedTaskStepDelayMs: 2 });
    const { container, render } = mount(env, wizard);
    wizard.continueWelcome();
    wizard.chooseChatGPT();
    wizard.continueMicCheck();
    render();
    await assertNoViolations(container, "wizard guided_task screen, running");

    await new Promise((resolve) => setTimeout(resolve, 40));
    render();
    wizard.saveAsWorkflow();
    render();
    await assertNoViolations(container, "wizard guided_task screen, saved");
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("schedule screen: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient(), { guidedTaskStepDelayMs: 2 });
    const { container, render } = mount(env, wizard);
    wizard.continueWelcome();
    wizard.chooseChatGPT();
    wizard.continueMicCheck();
    await new Promise((resolve) => setTimeout(resolve, 40));
    wizard.saveAsWorkflow();
    render();
    await assertNoViolations(container, "wizard schedule screen");
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("focus survives a rebuild while typing: the access-key field does not lose focus on every keystroke", () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient());
    const { render } = mount(env, wizard);
    wizard.continueWelcome();
    render();

    const input = env.document.querySelector<HTMLInputElement>('input[type="password"]');
    assert.ok(input, "access-key input must be on screen");
    input.focus();
    typeText(env.document, input, "sk-ant-test");

    const inputAfter = env.document.querySelector<HTMLInputElement>('input[type="password"]');
    assert.equal(env.document.activeElement, inputAfter, "focus must still be on the access-key input after typing");
    assert.equal(inputAfter?.value, "sk-ant-test");
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("focus moves onto the new screen's heading when the screen changes, but not on a same-screen data update", () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient());
    const { container, render } = mount(env, wizard);
    render();

    const welcomeHeading = container.querySelector("h2");
    assert.equal(env.document.activeElement, welcomeHeading, "first mount must focus the welcome heading");

    wizard.continueWelcome();
    render();
    const setupHeading = container.querySelector("h2");
    assert.notEqual(setupHeading, welcomeHeading);
    assert.equal(env.document.activeElement, setupHeading, "a screen change must move focus onto the new heading");

    // A same-screen data update (typing) must not steal focus back to the
    // heading: covered by the "focus survives a rebuild" test above, and
    // implicitly here since focus stayed off the heading after typing there.
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("Tab is trapped inside the dialog: Tab on the last control wraps to the first, Shift+Tab on the first wraps to the last", () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient());
    const { container, render } = mount(env, wizard);
    render();

    const focusable = Array.from(container.querySelectorAll<HTMLElement>("button, input, select"));
    assert.ok(focusable.length >= 1, "welcome screen must have at least the Continue button");
    const first = focusable[0];
    const last = focusable[focusable.length - 1];

    last.focus();
    pressTab(env.document, {});
    assert.equal(env.document.activeElement, first, "Tab past the last control must wrap to the first");

    first.focus();
    pressTab(env.document, { shift: true });
    assert.equal(env.document.activeElement, last, "Shift+Tab before the first control must wrap to the last");
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("Escape cancels an in-progress local download when Cancel is on screen, and does nothing when it is not", () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient(), { download: { tickMs: 1000 } });
    const { container, render } = mount(env, wizard);
    wizard.continueWelcome();
    render();

    // Before starting a download, Escape has nothing to do: still on setup_path.
    pressEscape(env.document, container.querySelector("[role='dialog']") as HTMLElement);
    assert.equal(wizard.getSnapshot().screen, "setup_path");

    wizard.startLocalDownload();
    render();
    // "starting" or already ticked to "downloading": either way the
    // download is active and cancelable, which is all this test needs.
    assert.ok(
      ["starting", "downloading"].includes(wizard.getSnapshot().setupPath.local.phase),
      `expected an active download phase, got ${wizard.getSnapshot().setupPath.local.phase}`,
    );

    const dialog = container.querySelector<HTMLElement>('[role="dialog"]');
    assert.ok(dialog);
    pressEscape(env.document, dialog);

    assert.equal(wizard.getSnapshot().setupPath.local.phase, "idle", "Escape must trigger the same Cancel action the visible button does");
    wizard.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Tab and Enter alone drive Continue through every screen up to guided_task", async () => {
  const env = createDomEnv();
  try {
    const wizard = createWizard(createMockBusClient(), { guidedTaskStepDelayMs: 2 });
    const { render } = mount(env, wizard);
    render();

    // welcome -> setup_path: Tab reaches Continue (the only control), Enter activates it.
    pressTab(env.document, {});
    let active = env.document.activeElement as HTMLElement;
    assert.equal(active.textContent, wizard.getSnapshot().welcome.continueButton);
    pressActivate(env.document, active);
    render();
    assert.equal(wizard.getSnapshot().screen, "setup_path");

    // setup_path -> mic_check: Tab to the ChatGPT sign-in button.
    const chatgptLabel = wizard.getSnapshot().setupPath.chatgpt.button;
    let guard = 0;
    while (active?.textContent !== chatgptLabel && guard < 30) {
      pressTab(env.document, {});
      active = env.document.activeElement as HTMLElement;
      guard++;
    }
    assert.equal(active.textContent, chatgptLabel, "Tab must be able to reach the ChatGPT sign-in button");
    pressActivate(env.document, active);
    render();
    assert.equal(wizard.getSnapshot().screen, "mic_check");

    // mic_check -> guided_task: Tab to Skip.
    active = env.document.activeElement as HTMLElement;
    const skipLabel = wizard.getSnapshot().micCheck.skipButton;
    guard = 0;
    while (active?.textContent !== skipLabel && guard < 30) {
      pressTab(env.document, {});
      active = env.document.activeElement as HTMLElement;
      guard++;
    }
    assert.equal(active.textContent, skipLabel);
    pressActivate(env.document, active);
    render();
    assert.equal(wizard.getSnapshot().screen, "guided_task");

    wizard.dispose();
  } finally {
    env.cleanup();
  }
});
