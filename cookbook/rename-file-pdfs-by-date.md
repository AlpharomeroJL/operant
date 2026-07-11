# Rename and file downloaded PDFs by date

For anyone who downloads lots of PDFs and needs to organize them into dated folders so they are easy to find later.

## Steps

1. Open File Explorer and go to your Downloads folder (or wherever your PDFs land).
2. Select all the PDF files you want to rename and organize (click the first one, hold Ctrl, click the others).
3. Right-click one of the selected files and open Properties to see the date each file was created or last modified.
4. Create new folders for each date or month in your Documents folder (e.g., "2024-01-January", "2024-02-February").
5. For each PDF, take note of its date and open it once to confirm it is the right file.
6. Rename each PDF to something more meaningful: include the date at the start and a description of what it is (e.g., "2024-01-15-Invoice-Acme-Corp.pdf").
7. Drag each renamed PDF into the matching date folder.
8. Go back to Downloads and confirm all PDFs are gone (they have been moved, not deleted).

*Benchmark: Yes*

## Inputs

| Name | What it is | Example |
|------|-----------|---------|
| Source folder | Where your PDFs currently are | C:\Users\Name\Downloads |
| File type | Always PDF for this workflow | .pdf |
| Folder structure | How to organize by time | By month (2024-01, 2024-02, etc.) |
| Naming pattern | How to rename files | [Date]-[Description].pdf |

## The workflow file goes here

Once the workflow is captured, the file will be saved as a workflow file that you can run again without retyping these steps.
