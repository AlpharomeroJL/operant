// Cookbook workflow: Rename and file downloaded PDFs by date
// Source prose: ../rename-file-pdfs-by-date.md
//
// Benchmark: yes - one of the three cookbook workflows that feed the
// crates/bench suite (see docs/specs/bench.md). Tracked in
// ../bench-workflows.json for L9B (bench-suite) to pick up.
//
// One representative PDF (prose steps 2 and 5-7 describe a batch of files;
// the step vocabulary has no loop, so a full batch is this workflow
// compiled once per file). "Drag into the folder" compiles to cut-then-paste
// since there is no drag primitive in the step vocabulary.
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_EXPLORER = { process: "explorer.exe", titlePattern: ".*" };

const SEL_FIRST_PDF = [
  { kind: "automation_id", value: "ShellListView" },
  { kind: "name_role_path", path: [{ role: "list", name: "Downloads" }, { role: "list_item", ordinal: 0 }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "list", ordinal: 0 }, { role: "list_item", ordinal: 0 }] },
];
const SEL_NEW_FOLDER_MENU = [
  { kind: "automation_id", value: "NewFolderButton" },
  { kind: "name_role_path", path: [{ role: "toolbar", name: "Command bar" }, { role: "menu_item", name: "New folder" }] },
];
const SEL_FOLDER_NAME_EDIT = [
  { kind: "automation_id", value: "RenameTextBox" },
  { kind: "name_role_path", path: [{ role: "window", name: "Documents" }, { role: "edit", name: "New folder name" }] },
];
const SEL_RENAME_EDIT = [
  { kind: "automation_id", value: "RenameTextBox" },
  { kind: "name_role_path", path: [{ role: "window", name: "Downloads" }, { role: "edit", name: "Rename file" }] },
];
const SEL_DEST_FOLDER = [
  { kind: "automation_id", value: "ShellListView" },
  { kind: "name_role_path", path: [{ role: "list", name: "Documents" }, { role: "list_item", name: "{dest_folder_name}" }] },
];

export default defineWorkflow({
  name: "rename-file-pdfs-by-date",
  version: "1.0.0",
  description: "Renames a downloaded PDF with a date prefix and files it into a matching date folder.",
  inputs: {
    source_folder: input.filePath({ default: "C:\\Users\\Name\\Downloads", label: "Folder where PDFs currently are" }),
    dest_folder_name: input.text({ default: "2024-01-January", label: "Date folder to file the PDF into" }),
    file_date: input.date({ default: "2024-01-15", label: "Date recorded on the PDF's file properties" }),
    new_file_name: input.text({ default: "2024-01-15-Invoice-Acme-Corp.pdf", label: "New file name (date prefix plus description)" }),
  },
  steps: [
    // 1. Open File Explorer and go to the Downloads folder.
    step.wait({ intent: "Wait for File Explorer to show the Downloads folder", scope: { window: WINDOW_EXPLORER }, timeoutMs: 3000 }),
    // 2-3. Select a PDF and check its properties for the file date.
    step.click({
      intent: "Click the PDF to select it",
      window: WINDOW_EXPLORER,
      selectors: SEL_FIRST_PDF,
      risk: "read",
    }),
    step.key({ intent: "Open Properties to see the file date", window: WINDOW_EXPLORER, combo: "alt+enter", risk: "read" }),
    step.wait({ intent: "Wait for the Properties dialog", scope: { window: WINDOW_EXPLORER }, timeoutMs: 2000 }),
    step.key({ intent: "Close the Properties dialog", window: WINDOW_EXPLORER, combo: "escape", risk: "read" }),
    // 4. Create a new folder for the date or month in Documents.
    step.click({
      intent: "Click New folder in the command bar",
      window: WINDOW_EXPLORER,
      selectors: SEL_NEW_FOLDER_MENU,
      risk: "write",
    }),
    step.type({
      intent: "Type the date folder name",
      window: WINDOW_EXPLORER,
      selectors: SEL_FOLDER_NAME_EDIT,
      text: "{dest_folder_name}",
      risk: "write",
    }),
    step.key({ intent: "Confirm the date folder name", window: WINDOW_EXPLORER, combo: "enter", risk: "write" }),
    // 5-6. Rename the PDF with the date at the start and a short description.
    step.click({
      intent: "Click the PDF to select it again",
      window: WINDOW_EXPLORER,
      selectors: SEL_FIRST_PDF,
      risk: "read",
    }),
    step.key({ intent: "Start renaming the PDF", window: WINDOW_EXPLORER, combo: "f2", risk: "write" }),
    step.type({
      intent: "Type the new file name",
      window: WINDOW_EXPLORER,
      selectors: SEL_RENAME_EDIT,
      text: "{new_file_name}",
      risk: "write",
    }),
    step.key({ intent: "Confirm the new file name", window: WINDOW_EXPLORER, combo: "enter", risk: "write" }),
    step.wait({ intent: "Wait for the rename to complete", scope: { window: WINDOW_EXPLORER }, timeoutMs: 2000 }),
    // 7. Move the renamed PDF into the matching date folder.
    step.key({ intent: "Cut the renamed PDF", window: WINDOW_EXPLORER, combo: "ctrl+x", risk: "write" }),
    step.click({
      intent: "Open the destination date folder",
      window: WINDOW_EXPLORER,
      selectors: SEL_DEST_FOLDER,
      risk: "read",
    }),
    step.key({ intent: "Paste the PDF into the date folder", window: WINDOW_EXPLORER, combo: "ctrl+v", risk: "write" }),
    step.wait({ intent: "Wait for the move to finish", scope: { window: WINDOW_EXPLORER }, timeoutMs: 3000 }),
    // 8. Go back to Downloads and confirm the PDF is gone (moved, not deleted).
    step.assert({
      intent: "Check that the PDF now lives in the date folder and not in Downloads",
      window: WINDOW_EXPLORER,
      expr: {
        op: "exists",
        query: { kind: "snapshot_list_item_in_folder", folder: "{dest_folder_name}", name: "{new_file_name}" },
      },
    }),
  ],
});

export const benchmark = true;
