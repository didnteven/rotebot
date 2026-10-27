function run() {
  const attempts = [
    () => new RegExp("a".repeat(5000)),
    () => RegExp("a".repeat(5000)),
    () => Reflect.construct(RegExp, ["a".repeat(5000)]),
    () => new (class extends RegExp {})("a".repeat(5000))
  ];
  let rejection;
  for (const construct of attempts) {
    let caught;
    try { construct(); } catch (error) { caught = error; }
    if (!(caught instanceof RangeError) || caught.message !== "regex pattern exceeds 1,000 characters") {
      throw new Error("expected bounded constructor rejection");
    }
    rejection = caught;
  }
  throw rejection;
}
