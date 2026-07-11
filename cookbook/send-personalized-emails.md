# Send personalized emails using a contact list

For marketing, HR, and sales teams who need to send similar emails to multiple people but with each person's name and details filled in.

## Steps

1. Open your spreadsheet file (Excel or Google Sheets) that contains the list of people to email. The spreadsheet should have columns for Name, Email Address, and any other details you want to personalize (Department, Project Name, etc.).
2. Draft the email message in a text editor or Word document. Use placeholders for the personalized parts: put [Name] where the person's name goes, [Email] where needed, or [ProjectName] for a custom field, etc.
3. Open your email application (Outlook, Gmail, or similar).
4. Look for a Mail Merge feature (in Outlook or Gmail) or do this manually if the feature is not available:
   - For Outlook: File > Start Mail Merge, or check Help for Mail Merge instructions specific to your version.
   - For Gmail: Use a Mail Merge extension from the Google Workspace Marketplace.
   - For other email apps: You may need to send emails one at a time using copy-paste.
5. If using Mail Merge: follow the prompts to connect your spreadsheet file, select the columns to use, and paste your drafted message with placeholders.
6. Review the preview to see how the message will look with the names and details filled in for one or two recipients.
7. If using Mail Merge: click Send All to send to everyone on the list.
8. If sending manually: For each row in the spreadsheet, create a new email, replace the placeholders with that person's information, and click Send.
9. Check your email Sent folder or the spreadsheet to confirm all emails were sent.

## Inputs

| Name | What it is | Example |
|------|-----------|---------|
| Contact list | Spreadsheet with names and email addresses | contacts.xlsx with columns: Name, Email, Department |
| Email template | Message text with placeholders for personalization | "Hi [Name], your project [ProjectName] is approved." |
| Personalization fields | Which columns from the spreadsheet to use | Name, Department, Project, Start Date |
| Email subject | Subject line (same for all or with personalization) | "Project Approval: [ProjectName]" |

## The workflow file

The compiled steps live in [`workflow.ts`](./send-personalized-emails/workflow.ts), written against `@operant/sdk`. Validated by `node cookbook/doctest.mjs`.
