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
};

export const paletteStrings = {
  placeholder: "Tell it what to do",
  submit: "Teach it",
  hint: "Press Enter to start teaching it from what's on screen right now",
};
