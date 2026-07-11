# Copy invoice rows from a web portal into a spreadsheet

For accounting teams who need to move invoice data from a vendor or customer portal into Excel without retyping.

## Steps

1. Open the web portal in your browser and log in.
2. Navigate to the invoices page and filter or search for the invoices you need.
3. On the spreadsheet (already open in Excel), place your cursor in the first cell where you want the data to start.
4. Go back to the portal and select the invoice rows you want to copy (highlight the first row, hold Shift, then click the last row).
5. Copy the selected rows (Ctrl+C).
6. Go back to the spreadsheet and paste the rows (Ctrl+V).
7. Check that all columns lined up correctly and no data was cut off.
8. Delete any extra blank rows at the bottom.
9. Save the spreadsheet (Ctrl+S).

*Benchmark: Yes*

## Inputs

| Name | What it is | Example |
|------|-----------|---------|
| Portal URL | Web address where your invoices live | vendor.com/invoices |
| Invoice filter | Column or field to search by (date range, status, etc.) | Invoices from last month |
| Spreadsheet file | Excel file open and ready to receive data | 2024-Q3-invoices.xlsx |
| Start cell | Row and column where you want data to begin | A5 (row 5, column A) |

## The workflow file goes here

Once the workflow is captured, the file will be saved as a workflow file that you can run again without retyping these steps.
