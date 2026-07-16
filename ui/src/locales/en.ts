// English locale catalog. Extracted from ui/src/wizard/strings.ts and palette UI.
// All strings must use only user-facing vocabulary from contracts/microcopy_glossary.json.

export const welcomeStrings = {
  heading: "Welcome to Operant",
  body: "Operant learns the things you do on your computer so it can do them for you.",
  continueButton: "Continue",
};

export const setupPathStrings = {
  heading: "How should Operant think?",
  subheading: "Pick how Operant gets its thinking power. You can change this later in settings.",
  demoLink: "Just show me a demo",
  cards: {
    chatgpt: {
      title: "Sign in with ChatGPT",
      body: "Use the AI plan you already pay for. No keys to copy.",
      button: "Sign in with ChatGPT",
    },
    claude: {
      title: "Sign in with Claude",
      body: "Use the AI plan you already pay for. No keys to copy.",
      button: "Sign in with Claude",
    },
    local: {
      title: "Download a free brain",
      body: "Get a model that runs right on this computer. Free and private, and it keeps working without the internet once it is downloaded.",
      sizeLabel: (size: string) => `This download is about ${size}.`,
      diskCheckLabel: "Checking free space on this computer.",
      diskCheckOk: "You have enough room for this.",
      diskCheckLow: (needed: string) => `This computer is short on space. Free up ${needed} and try again.`,
      compatCheckLabel: "Checking whether this computer can run it well.",
      compatCheckOk: "This computer can run it well.",
      compatCheckSlow: "This computer can run it, but it may think slowly.",
      compatCheckFail: "This computer does not have enough graphics memory for this option. Try signing in with ChatGPT or Claude instead.",
      button: "Download",
      continueButton: "Continue",
      download: {
        starting: "Getting ready to download.",
        downloading: (percent: number) => `Downloading, ${percent} percent done.`,
        resuming: "Picking up where it left off.",
        paused: "Paused. What is already downloaded is saved.",
        pauseButton: "Pause",
        resumeButton: "Resume",
        cancelButton: "Cancel download",
        retryButton: "Try again",
        verifying: "Double-checking the download.",
        complete: "Ready to go.",
        failed: "The download did not finish.",
      },
    },
    accessKey: {
      title: "I have an access key",
      body: "Paste the access key you already have. Operant will figure out where it is from.",
      placeholder: "Paste your access key here",
      providerLabel: "Which service is this key from?",
      providerAutoDetected: (provider: string) => `We recognized this key. It looks like it is from ${provider}.`,
      providerPickManually: "We could not tell where this key is from. Pick it from the list below.",
      button: "Continue",
    },
  },
};

export const providerDisplayNames = {
  chatgpt: "ChatGPT",
  claude: "Claude",
};

export const micCheckStrings = {
  heading: "Let's check your microphone",
  body: "Say something and watch the level meter move. This makes sure Operant can hear you when you talk to it.",
  sampleButton: "Play a sample",
  levelMeterLabel: "Microphone level",
  skipButton: "Skip for now",
  continueButton: "Sounds good",
  failed: "We could not hear anything. Check that your microphone is connected and try again.",
};

export const guidedTaskStrings = {
  headingReal: "Let's try your first task",
  introReal: "Watch Operant fill out a sample invoice on the practice page. This teaches Operant by showing it what to do, step by step.",
  headingDemo: "Watching a quick demo",
  introDemo: "Here is Operant doing a task with nothing set up yet. Nothing here can read, change, or send anything of yours.",
  runningLabel: "Working on it",
  doneLabel: "Done. Here is everything it just did.",
  saveButton: "Save as workflow",
  savedHint: "Saved. Operant can do this again anytime from your saved list.",
  demoContinueButton: "Set it up for real",
  demoContinueHint: "Ready to connect it to your own thinking power?",
};

export const scheduleStrings = {
  heading: "Want this to run by itself?",
  body: "Choose when Operant should do this on its own, or run it yourself whenever you like.",
  options: {
    manual: "Only when I click run",
    daily: "Every day",
    weekly: "Every week",
    when_file_changes: "When a file changes",
    when_app_opens: "When an app opens",
    when_email_arrives: "When an email arrives",
  },
  continueButton: "Save this schedule",
};

export const downloadErrorStrings = {
  checksum_mismatch: {
    what: "The downloaded file did not match what we expected.",
    why: "This can happen if the download was interrupted or the connection was not trustworthy.",
    action: "Try downloading again.",
  },
  network_error: {
    what: "The download could not reach the server.",
    why: "This usually means the internet connection dropped.",
    action: "Check your connection and try again.",
  },
  disk_space: {
    what: "There was not enough room to save the download.",
    why: "This computer is low on free space.",
    action: "Free up some space and try again.",
  },
  not_found: {
    what: "The download could not find the file it was looking for.",
    why: "The link to this file may be out of date.",
    action: "Try again later, or pick a different option.",
  },
};

export const wizardShellStrings = {
  dialogLabel: "Get started with Operant",
  stepLabel: (n: number, total: number) => `Step ${n} of ${total}`,
  // D5 (docs/specs/design.md section 3.3): the accessible name for the
  // three-quiet-dots progress indicator (WCAG 4.1.2 / axe's
  // aria-progressbar-name: a progressbar needs a name distinct from the
  // value it reports via aria-valuetext, same reasoning as the local-model
  // download bar's own aria-labelledby a few lines below in this file).
  progressLabel: "Setup progress",
};

// Copy for the engine-config confirmation the mic-check screen shows once a
// setup path has written real config (ui/src/wizard/engine.ts). The probe
// lines stay honest: probe_backend is not-yet-implemented in the contract, so
// the not-yet-checked wording never claims a working connection. Every string
// uses only user-facing vocabulary from contracts/microcopy_glossary.json
// (note: "model", never the internal word for it).
export const engineStatusStrings = {
  confirm: (name: string) => `You're set up with ${name}.`,
  names: {
    chatgpt: "ChatGPT",
    claude: "Claude",
    local: "the model on this computer",
  },
  probe: {
    checking: "Checking the connection.",
    reachable: "The connection looks good.",
    unreachable: "We could not reach it, but you can keep going and set this up later.",
    // Honest not-yet-implemented / probe-unavailable wording: never a green result.
    not_implemented: "We could not check the connection yet.",
    unavailable: "We could not check the connection yet.",
  },
};

export const paletteStrings = {
  placeholder: "Tell it what to do",
  submit: "Teach it",
  hint: "Press Enter to start teaching it from what's on screen right now",
  // D3's floating overlay (docs/specs/design.md section 3, Palette). Appended
  // rather than replacing anything above: placeholder/hint still do the same
  // job they always did (the input's placeholder, and the teach row's own
  // tooltip below), submit is kept even though the overlay has no separate
  // submit button anymore (Enter drives everything), append-only during the
  // campaign.
  overlayLabel: "Command palette",
  groupWorkflows: "Workflows",
  groupActions: "Actions",
  groupRecent: "Recent",
  // design.md section 3: "Typing a sentence that matches nothing offers
  // 'Teach this' as the amber primary row," quoted verbatim.
  teachThis: "Teach this",
  // Footer hints (design.md section 3: "Enter to run, Ctrl+Enter to dry run,
  // Tab for details"). "Preview" here, not that middle phrase verbatim:
  // contracts/microcopy_glossary.json maps that same internal concept to
  // the user-facing word "preview", the same word this catalog would use
  // anywhere else it came up.
  footerRun: "Enter to run",
  footerPreview: "Ctrl+Enter to preview",
  footerDetails: "Tab for details",
  openScreenHint: "Switch to this screen",
  themeActionTitle: "Theme",
  settingsHint: "Open Settings",
};

// Target-app picker (ADR 0003, A1): the step after "Teach this" where the
// person picks which open app to teach against, so a teach run never lands on
// Operant itself (the foreground window while the palette is up). Plain,
// verb-first copy; the front-app row is the pre-selected smart default.
export const targetAppStrings = {
  overlayLabel: "Choose an app to teach",
  heading: "Pick the app to teach",
  frontApp: "Use the app I have in front (not Operant)",
  loading: "Looking for your open apps",
  empty: "No other apps are open. Open the app you want to teach, then try again.",
  confirmHint: "Enter to teach this app",
  cancelHint: "Esc to go back",
};
