# Extract totals from a scanned receipt

For bookkeepers and expense managers who need to pull dollar amounts and dates from receipt images without retyping them.

## Steps

1. Open the receipt image or PDF on your computer (it may be a photo you took with your phone, or a scan from a scanner).
2. Open the spreadsheet file where you track expenses (in Excel or Google Sheets).
3. Place your cursor in the cell where you want to enter the data (start with date, then amount, then description).
4. Look at the receipt image and find the date. Type or paste it into the spreadsheet cell.
5. Tab or click to the next cell and find the total dollar amount on the receipt. Type or paste it.
6. In the next cell, type a short description of what the receipt is for (e.g., "Office supplies", "Lunch meeting", "Software subscription").
7. If the receipt shows tax and subtotal separately, add those as separate cells if needed.
8. Move to the next row and repeat steps 4-7 for the next receipt, or go to step 9 if this was the last one.
9. Once all receipts are entered, save the spreadsheet (Ctrl+S).

## Inputs

| Name | What it is | Example |
|------|-----------|---------|
| Receipt format | Image or PDF of the receipt | JPG photo or PDF scan |
| Data to extract | Which numbers you need | Date, subtotal, tax, total amount |
| Spreadsheet file | Excel or Google Sheets file for expenses | Expenses-2024.xlsx |
| Category field | Optional: how to categorize the expense | Travel, Meals, Office, etc. |

## The workflow file

The compiled steps live in [`workflow.ts`](./extract-receipt-totals/workflow.ts), written against `@operant/sdk`. Validated by `node cookbook/doctest.mjs`.
