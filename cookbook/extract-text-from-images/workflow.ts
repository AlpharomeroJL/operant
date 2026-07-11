// Cookbook workflow: Extract and format text from images for a document
// Source prose: ../extract-text-from-images.md
//
// Benchmark: yes - one of the three cookbook workflows that feed the
// crates/bench suite (see docs/specs/bench.md). Tracked in
// ../bench-workflows.json for L9B (bench-suite) to pick up.
//
// One representative image (prose step 7, "move to the next image and
// repeat", is a re-run of this workflow with a new image_file/extracted_text
// input, not a loop inside the compiled trace).
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_VIEWER = { process: "photos.exe", titlePattern: ".*" };
const WINDOW_DOC = { process: "winword.exe", titlePattern: ".* - Word" };

const SEL_DOC_BODY = [
  { kind: "automation_id", value: "DocumentBody" },
  { kind: "name_role_path", path: [{ role: "window", name: "Document1 - Word" }, { role: "document", name: "Document body" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
];

export default defineWorkflow({
  name: "extract-text-from-images",
  version: "1.0.0",
  description: "Reads text out of an image and types it into a document, preserving the original structure.",
  inputs: {
    image_file: input.filePath({ default: "whiteboard-photo-2024-01-15.jpg", label: "Image containing text to extract" }),
    output_document: input.filePath({ default: "Whiteboard-notes-2024-01-15.docx", label: "Destination document" }),
    extracted_text: input.text({
      default: "- Q1 roadmap review\n- Ship the OCR adapter [unclear: 'by end of month'?]\n- Follow up with design",
      label: "Text transcribed from the image",
    }),
  },
  steps: [
    // 1-3. Open the first image and read the text and its structure.
    step.wait({ intent: "Wait for the image to open", scope: { window: WINDOW_VIEWER }, timeoutMs: 5000 }),
    // 4. Open a blank document where the extracted text should go.
    step.key({ intent: "Switch to the destination document", window: WINDOW_DOC, combo: "alt+tab", risk: "read" }),
    step.click({
      intent: "Click the document body",
      window: WINDOW_DOC,
      selectors: SEL_DOC_BODY,
      risk: "read",
    }),
    // 5-6. Type the extracted text, keeping its structure, and bracket any unclear words.
    step.type({
      intent: "Type the text extracted from the image",
      window: WINDOW_DOC,
      selectors: SEL_DOC_BODY,
      text: "{extracted_text}",
      risk: "write",
    }),
    step.wait({ intent: "Wait for the text to render", scope: { window: WINDOW_DOC }, timeoutMs: 2000 }),
    // 8. Read through the document once to check for typos.
    step.scroll({ intent: "Scroll through the document to proofread it", window: WINDOW_DOC, direction: "down", amount: 5, risk: "read" }),
    step.assert({
      intent: "Check that the extracted text appears in the document",
      window: WINDOW_DOC,
      expr: {
        op: "contains",
        query: { kind: "snapshot_element_value", role: "document", name: "Document body" },
        value: "{extracted_text}",
      },
    }),
    // 9. Save the document with a clear name.
    step.key({ intent: "Save the document", window: WINDOW_DOC, combo: "ctrl+s", risk: "write" }),
    step.wait({ intent: "Wait for the save dialog to appear", scope: { window: WINDOW_DOC }, timeoutMs: 3000 }),
    step.type({
      intent: "Type the document file name",
      window: WINDOW_DOC,
      selectors: SEL_DOC_BODY,
      text: "{output_document}",
      risk: "write",
    }),
    step.key({ intent: "Confirm the save", window: WINDOW_DOC, combo: "enter", risk: "write" }),
    step.wait({ intent: "Wait for the save to complete", scope: { window: WINDOW_DOC }, timeoutMs: 5000 }),
    step.assert({
      intent: "Check that the document was saved with the expected name",
      window: WINDOW_DOC,
      expr: {
        op: "matches",
        query: { kind: "snapshot_window_title" },
        regex: "^{output_document} - Word$",
      },
    }),
  ],
});

export const benchmark = true;
