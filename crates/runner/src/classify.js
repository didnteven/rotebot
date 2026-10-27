// Capture trusted operations before user code. Never read exception.message or
// call its toString: either can be a hostile getter/function after the deadline.
(() => {
  const own = Object.getOwnPropertyDescriptor;
  const proto = Object.getPrototypeOf;
  const syntax = SyntaxError.prototype;
  const includes = String.prototype.includes;
  const apply = Reflect.apply;
  return e => {
    // QuickJS throws null when even allocating the out-of-memory Error fails
    // (JS_ThrowError2). Classify that sentinel without allocating a descriptor.
    if (e === null) return "memory";
    const object = e && typeof e === "object";
    const descriptor = object ? own(e, "message") : null;
    const value = descriptor ? own(descriptor, "value") : null;
    const message = value && typeof value.value === "string" ? value.value : "";
    return apply(includes, message, ["out of memory"]) ? "memory" :
      (object && proto(e) === syntax ? "syntax" : "runtime");
  };
})()
