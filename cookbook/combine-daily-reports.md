# Download daily reports and combine them into one file

For managers and analysts who need to collect daily reports from a website and merge them into a single file for weekly or monthly review.

## Steps

1. Open your web browser and go to the reports portal or website where daily reports are published.
2. Log in if needed.
3. Look for a date picker or calendar and select the first date you want to download.
4. Find and click the Download button (or similar) to save the daily report to your computer. It will likely go to your Downloads folder.
5. Repeat steps 3-4 for each additional date you need, downloading one report per day.
6. Once all daily reports are downloaded, open your spreadsheet application (Excel or Google Sheets).
7. Create a new blank spreadsheet or open an existing one where you want to combine all the data.
8. Open the first downloaded report. Copy all the data (Ctrl+A to select all, then Ctrl+C to copy).
9. In your combined spreadsheet, click on the first cell and paste the data (Ctrl+V).
10. For each additional daily report, copy the data and paste it into new rows below the previous data (or on a new sheet if you prefer to keep days separate).
11. Once all reports are combined, add a header row at the top with column names if it is missing.
12. Save the combined file with a name that indicates it contains multiple days (e.g., "Weekly-Report-2024-01-08-to-2024-01-14.xlsx").

## Inputs

| Name | What it is | Example |
|------|-----------|---------|
| Reports portal | Website where daily reports are found | analytics.company.com/reports |
| Date range | Which dates you need to download | January 8 through January 14, 2024 |
| Report format | File type that downloads | CSV, Excel (.xlsx), or PDF |
| Output file | Where to save the combined report | C:\Users\Name\Documents\Weekly-Reports |

## The workflow file

The compiled steps live in [`workflow.ts`](./combine-daily-reports/workflow.ts), written against `@operant/sdk`. Validated by `node cookbook/doctest.mjs`.
