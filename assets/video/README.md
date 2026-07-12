# assets/video

Launch demo video for v1.0.0 (packet C22, FR-D6), assembled from the real
V1 capture assets in `assets/`.

## What is here

- `launch-demo.mp4`: the assembled demo. 1280x720, 25fps, about 57 seconds,
  silent, with on-screen captions burned in via ffmpeg drawtext. Sequenced
  from the thirteen capture assets in the order given by LAUNCH.md section
  4's shot list. Each GIF is looped enough times to reach a readable beat
  (once to three times depending on its native length), each PNG is held
  static for a few seconds. About 1.1 MB, well under the 60 MB budget.
- `narration-script.md`: the spoken script the captions are drawn from,
  timed to each shot, written to be rendered by Operant's own local TTS
  voice per FR-D6.

## Why the video is silent

FR-D6 calls for a narrated video. The voice sidecar (`sidecars/voice`) ships
two TTS providers behind one interface:

- `src/providers/mockProvider.js`, wired up today. Its `tts()` calls
  `buildFakeWav(text)`, a synthetic wav built from the text, not real
  speech.
- `src/providers/kokoroProvider.js`, a documented seam. Its `tts()` throws
  `NotImplementedError("Kokoro tts(): Kokoro wiring is a documented seam,
  not implemented yet")`.

Recording a voice track today would mean shipping the mock's fake wav as if
it were narration. That is not narration, it is noise with the right
duration, and playing it back in a launch video would be actively
misleading. So this video ships silent, with the narration lines burned in
as captions instead of a fake voice track standing in for one.

## What real narration needs

1. Wire an actual Kokoro ONNX or PyTorch runtime into
   `sidecars/voice/src/providers/kokoroProvider.js`. The file already
   documents the four steps: add the runtime as an optional dependency
   loaded inside the first `tts()` call, resolve the configured voice and
   speaking rate, synthesize the text to 16-bit PCM, and wrap it in the same
   WAV framing `wav.js` already produces for the mock so callers do not need
   to branch on which provider produced the audio.
2. Feed each line in `narration-script.md` to `tts()` in shot order and
   concatenate the resulting audio against the timestamps in that file.
3. Mux the audio onto `launch-demo.mp4`, or re-cut the video against the
   real speech timing, since spoken pacing will not exactly match the
   silent on-screen beats used here.

## The one placeholder asset

Twelve of the thirteen source assets are real captures of the running UI.
`assets/10-undo.gif` is not: it is a labeled placeholder graphic (see
`assets/alt-text.md` and the Capture TODO at the bottom of `LAUNCH.md`),
because no undo screen exists in `ui/src` yet. It is still included here, in
its place in the shot list, but its caption uses a distinct orange warning
style and says plainly that it is a placeholder rather than narrating it as
a working feature. Once a real undo screen ships and `10-undo.gif` is
recaptured, redo this shot and its line in `narration-script.md` from the
real capture.
