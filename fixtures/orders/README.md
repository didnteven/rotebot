# Fabricated orders task

All names, IDs, dates and amounts here are invented. These files contain no real
customer or account data. `task.json` records the confirmed rules and finite mapping.

`input.csv` has 18 rows with MM/DD/YY dates (2000–2099), USD decimal strings, a
quoted customer name containing a comma, and the formula-looking customer
`=SUM(A1:A2)`. `expected.csv` is an independently authored oracle: exactly
`Order,Date,Amount`, ISO calendar dates, and plain decimal amounts with two places.
The customer column is never mapped, so neither the name nor formula should leak
into the output. A formula-looking value that *does* reach output must receive a
leading apostrophe; this fixture does not call for an apostrophe anywhere in output.

Every negative input must stop and explain before publishing any output:

| File | Reason |
| --- | --- |
| `negative/bad-date.csv` | `13/45/26` is not a calendar date. |
| `negative/missing-amount.csv` | A required amount cell is empty. |
| `negative/missing-column.csv` | The required `created` column is absent. |
| `negative/empty.csv` | The file has only a header and no data rows. |

The foundation checks fixture shape, contract deserialization and schema validity.
Execution and behavioral acceptance of these cases belong to the later host-owned
CSV interpreter/broker task; this repository does not yet run transformations.
