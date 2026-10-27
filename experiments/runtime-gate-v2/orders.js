function run() {
  const rows = macroscope.csv.parse(macroscope.input("input_csv"), {required: ["order_id", "created", "amount", "customer"]});
  const output = rows.map(r => ({
    Order: r.order_id,
    Date: macroscope.date.toISO(r.created, "MM/DD/YY"),
    Amount: macroscope.money.decimal(r.amount, {currency: "USD", decimals: 2})
  }));
  macroscope.output("orders_ready.csv", macroscope.csv.stringify(output, ["Order", "Date", "Amount"]));
  macroscope.log("rows: " + rows.length);
}
