(native => {
  const stringify = JSON.stringify;
  const keys = Object.keys;
  const descriptor = Object.getOwnPropertyDescriptor;
  const prototype = Object.getPrototypeOf;
  const plain = Object.prototype;
  const array = Array.isArray;
  const finite = Number.isFinite;
  return (connectorId, toolName, args) => {
    let nodes = 0;
    const seen = new Set();
    const validate = (v, depth) => {
      if (++nodes > 65536 || depth > 64) throw new TypeError("JSON args too complex");
      if (v === null || typeof v === "string" || typeof v === "boolean") return;
      if (typeof v === "number" && finite(v)) return;
      if (typeof v !== "object" || seen.has(v)) throw new TypeError("Args must be JSON");
      if (!array(v) && prototype(v) !== plain && prototype(v) !== null) throw new TypeError("Args must be plain JSON");
      seen.add(v);
      if (array(v) && keys(v).length !== v.length) throw new TypeError("Sparse arrays are not JSON");
      for (const k of keys(v)) {
        const d = descriptor(v, k);
        if (!d || !("value" in d) || k === "toJSON") throw new TypeError("Args must be JSON data");
        validate(d.value, depth + 1);
      }
      seen.delete(v);
    };
    validate(args, 0);
    native(connectorId, toolName, stringify(args));
  };
})
