// Cookbook workflow: Organize files by type and move them into folders
// Source prose: ../organize-files-by-type.md
// Benchmark: no
//
// The step vocabulary has no drag primitive, so "drag the file into the
// folder" (prose step 5) compiles to cut-then-paste (ctrl+x, navigate,
// ctrl+v), which is keyboard-equivalent to a drag in File Explorer. One
// representative file (a PDF) stands in for "repeat steps 4-5 until all
// files have been sorted."
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_EXPLORER = { process: "explorer.exe", titlePattern: ".*" };

const SEL_NEW_FOLDER_MENU = [
  { kind: "automation_id", value: "NewFolderButton" },
  { kind: "name_role_path", path: [{ role: "toolbar", name: "Command bar" }, { role: "menu_item", name: "New folder" }] },
];
const SEL_FOLDER_NAME_EDIT = [
  { kind: "automation_id", value: "RenameTextBox" },
  { kind: "name_role_path", path: [{ role: "window", name: "Archive" }, { role: "edit", name: "New folder name" }] },
];
const SEL_FIRST_FILE = [
  { kind: "automation_id", value: "ShellListView" },
  { kind: "name_role_path", path: [{ role: "list", name: "Archive" }, { role: "list_item", ordinal: 0 }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "list", ordinal: 0 }, { role: "list_item", ordinal: 0 }] },
];
const SEL_DEST_FOLDER = [
  { kind: "automation_id", value: "ShellListView" },
  { kind: "name_role_path", path: [{ role: "list", name: "Archive" }, { role: "list_item", name: "{folder_name}" }] },
];

export default defineWorkflow({
  name: "organize-files-by-type",
  version: "1.0.0",
  description: "Creates a type-named folder and files one matching file into it.",
  inputs: {
    source_folder: input.filePath({ default: "C:\\Users\\Name\\Documents\\Archive", label: "Folder with mixed files to organize" }),
    folder_name: input.text({ default: "PDFs", label: "Name of the destination subfolder" }),
  },
  steps: [
    // 1. Open File Explorer and go to the folder that needs organizing.
    step.wait({ intent: "Wait for File Explorer to show the source folder", scope: { window: WINDOW_EXPLORER }, timeoutMs: 3000 }),
    // 2-3. Create a new folder for the file type.
    step.click({
      intent: "Click New folder in the command bar",
      window: WINDOW_EXPLORER,
      selectors: SEL_NEW_FOLDER_MENU,
      risk: "write",
    }),
    step.type({
      intent: "Type the new folder name",
      window: WINDOW_EXPLORER,
      selectors: SEL_FOLDER_NAME_EDIT,
      text: "{folder_name}",
      risk: "write",
    }),
    step.key({ intent: "Confirm the new folder name", window: WINDOW_EXPLORER, combo: "enter", risk: "write" }),
    step.wait({ intent: "Wait for the folder to be created", scope: { window: WINDOW_EXPLORER }, timeoutMs: 2000 }),
    // 4. Click the first file and check its type by extension.
    step.click({
      intent: "Click the first file to check its type",
      window: WINDOW_EXPLORER,
      selectors: SEL_FIRST_FILE,
      risk: "read",
    }),
    // 5. Move that file into the matching folder (cut, open folder, paste).
    step.key({ intent: "Cut the selected file", window: WINDOW_EXPLORER, combo: "ctrl+x", risk: "write" }),
    step.click({
      intent: "Open the destination folder",
      window: WINDOW_EXPLORER,
      selectors: SEL_DEST_FOLDER,
      risk: "read",
    }),
    step.key({ intent: "Paste the file into the destination folder", window: WINDOW_EXPLORER, combo: "ctrl+v", risk: "write" }),
    step.wait({ intent: "Wait for the move to finish", scope: { window: WINDOW_EXPLORER }, timeoutMs: 3000 }),
    // 8. Confirm the folder structure looks correct.
    step.scroll({ intent: "Scroll through the organized folder to review it", window: WINDOW_EXPLORER, direction: "down", amount: 3, risk: "read" }),
    step.assert({
      intent: "Check that the destination folder now contains the moved file",
      window: WINDOW_EXPLORER,
      expr: {
        op: "exists",
        query: { kind: "snapshot_list_item_in_folder", folder: "{folder_name}" },
      },
    }),
  ],
});

export const benchmark = false;
