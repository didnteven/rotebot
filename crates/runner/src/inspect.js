// Capture intrinsics before user code. Read only own data descriptors: never
// invoke getters, toString, or user-controlled name/prototype properties.
(() => {
  const own = Object.getOwnPropertyDescriptor;
  const proto = Object.getPrototypeOf;
  const apply = Reflect.apply;
  const slice = String.prototype.slice;
  // QuickJS-NG stores native stacks in [[ErrorData]], exposed by this
  // intrinsic getter. Calling it directly cannot invoke an overridden getter.
  const getStack = own(Error.prototype, "stack").get;
  const prototypes = [Error.prototype, TypeError.prototype, ReferenceError.prototype,
    SyntaxError.prototype, RangeError.prototype, InternalError.prototype];
  const names = ["Error", "TypeError", "ReferenceError", "SyntaxError", "RangeError", "InternalError"];
  const data = (e, key) => {
    const d = own(e, key);
    const v = d ? own(d, "value") : undefined;
    return v ? v.value : undefined;
  };
  const text = (e, key) => {
    const v = key === "stack" && !own(e, key) ? apply(getStack, e, []) : data(e, key);
    return typeof v === "string" ? apply(slice, v, [0, 4096]) : "";
  };
  const number = (e, key) => {
    const v = data(e, key);
    return typeof v === "number" && v > 0 && v < 1000000000 && v % 1 === 0 ? v : null;
  };
  return e => {
    let name = "Error";
    let p = proto(e);
    // Bound traversal even for unusual user-defined Error subclasses.
    for (let depth = 0; p && depth < 16; depth++, p = proto(p)) {
      for (let i = 0; i < prototypes.length; i++) {
        if (p === prototypes[i]) {
          name = names[i];
          return [name, text(e, "message"), text(e, "stack"),
            number(e, "lineNumber"), number(e, "columnNumber"), text(e, "fileName")];
        }
      }
    }
    return [name, "", "", null, null, ""];
  };
})()
