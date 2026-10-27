function run() {
  const parts = "alpha,  beta ,gamma".split(/\s*,\s*/);
  const replaced = "one   two\tthree".replace(/\s+/g, " ");
  const valid = /^[A-Z]{2}-\d+$/.test("AB-123");
  const invalid = /^[A-Z]{2}-\d+$/.test("ab-123");
  macroscope.output("regex.txt", JSON.stringify({parts, replaced, valid, invalid}) + "\n");
}
