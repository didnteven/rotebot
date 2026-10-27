(native, declared) => {
  const stringify = JSON.stringify;
  const parse = JSON.parse;
  const keys = Object.keys;
  const descriptor = Object.getOwnPropertyDescriptor;
  const prototype = Object.getPrototypeOf;
  const plain = Object.prototype;
  const array = Array.isArray;
  const finite = Number.isFinite;
  return (...args) => {
    let prompt;
    let data;
    let schema;
    if (args.length === 2) {
      if (typeof declared !== "string" || declared === "") throw new TypeError("Model step not declared; rebuild and approve what is sent");
      [data, schema] = args;
      prompt = declared;
    } else if (args.length === 3) {
      [prompt, data, schema] = args;
    } else {
      throw new TypeError("Model ask needs (data, schema) or (prompt, data, schema)");
    }
    if (typeof prompt !== "string") throw new TypeError("Prompt must be text");
    let nodes = 0;
    const seen = new Set();
    const validate = (v, depth) => {
      if (++nodes > 65536 || depth > 64) throw new TypeError("Model JSON too complex");
      if (v === null || typeof v === "string" || typeof v === "boolean") return;
      if (typeof v === "number" && finite(v)) return;
      if (typeof v !== "object" || seen.has(v)) throw new TypeError("Model arguments must be JSON data");
      if (!array(v) && prototype(v) !== plain && prototype(v) !== null) throw new TypeError("Model arguments must be plain JSON");
      seen.add(v);
      if (array(v) && keys(v).length !== v.length) throw new TypeError("Sparse arrays are not JSON");
      for (const k of keys(v)) {
        const d = descriptor(v, k);
        if (!d || !("value" in d) || k === "toJSON") throw new TypeError("Model arguments must be JSON data");
        validate(d.value, depth + 1);
      }
      seen.delete(v);
    };
    validate(data, 0);
    validate(schema, 0);
    return parse(native(stringify({prompt, data, schema})));
  };
}
