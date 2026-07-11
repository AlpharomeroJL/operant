# Reply to a canned customer email

For customer service and support teams who need to send the same response to multiple similar requests without typing it each time.

## Steps

1. Open your email application and go to your inbox.
2. Find the first email you need to reply to.
3. Click Reply (or Reply All if you need to include everyone on the original email).
4. In the reply field, type or paste the response text that should be sent. You can use a template document on your desktop or in a note app if you want to copy from it.
5. If the response needs to include the person's name or any unique detail, add that information now (find it from their email signature or previous messages).
6. Before sending, read the reply once to make sure it makes sense and answers the question.
7. Click Send.
8. Go back to the inbox and find the next email that needs the same reply.
9. Repeat steps 2 through 7 for each similar email.
10. When you have sent all replies, move or delete the original emails so you know they have been handled.

## Inputs

| Name | What it is | Example |
|------|-----------|---------|
| Email inbox | Your email account and folder | Outlook inbox or Gmail inbox |
| Message template | Text to send in replies | "Thank you for contacting us. Your request has been received and will be handled within 24 hours." |
| Custom fields | Information unique to each person | Their name, order number, or issue type |
| Recipient list | How many people need this reply | 5-10 similar emails in the inbox |

## The workflow file

The compiled steps live in [`workflow.ts`](./reply-canned-customer-email/workflow.ts), written against `@operant/sdk`. Validated by `node cookbook/doctest.mjs`.
