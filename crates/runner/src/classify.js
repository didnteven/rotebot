// Capture trusted operations before user code. Never read exception.message or
// call its toString: either can be a hostile getter/function after the deadline.
(() => {
  const own = Object.getOwnPropertyDescriptor;
  const proto = Object.getPrototypeOf;
  const syntax = SyntaxError.prototype;
  const includes = String.prototype.includes;
  return e => {
    const object = e && typeof e === "object";
    const descriptor = object ? own(e, "message") : null;
    const message = descriptor && typeof descriptor.value === "string" ? descriptor.value : "";
    return includes.call(message, "out of memory") ? "memory" :
      (object && proto(e) === syntax ? "syntax" : "runtime");
  };
})()
