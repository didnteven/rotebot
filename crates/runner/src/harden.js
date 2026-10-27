"use strict";
(() => {
  const nativeCall = Function.prototype.call.bind(Function.prototype.call);
  // Constructors remain reachable through prototype chains unless these links
  // are removed too (including the async and generator function families).
  for (const f of [function(){}, async function(){}, function*(){}, async function*(){}]) {
    const p = Object.getPrototypeOf(f);
    Object.defineProperty(p, "constructor", {value: undefined, writable: false, configurable: false});
    Object.freeze(p);
  }
  for (const name of ["eval", "Function", "AsyncFunction", "GeneratorFunction",
      "AsyncGeneratorFunction", "Worker", "SharedArrayBuffer", "Atomics", "Promise", "queueMicrotask"]) {
    Object.defineProperty(globalThis, name, {value: undefined, writable: false, configurable: false});
  }
  const NativeRegExp = RegExp;
  const prototype = NativeRegExp.prototype;
  const source = Object.getOwnPropertyDescriptor(prototype, "source").get;
  const toString = String;
  const iterator = String.prototype[Symbol.iterator];
  const next = Object.getPrototypeOf(iterator.call("")).next;
  // Capture primitives before user code can replace String/iterator methods.
  function boundedRegExp(pattern, flags) {
    let text;
    if (pattern !== undefined) {
      try { text = nativeCall(source, pattern); }
      catch (_) { text = toString(pattern); }
      let count = 0;
      const chars = nativeCall(iterator, text);
      while (!nativeCall(next, chars).done) {
        if (++count > 1000) throw new RangeError("regex pattern exceeds 1,000 characters");
      }
    }
    return NativeRegExp(pattern === undefined ? undefined : text,
      flags === undefined && pattern instanceof NativeRegExp ? pattern.flags : flags);
  }
  // Remove every legacy static, including aliases ($_, $&, $+, $`, $').
  for (const key of Object.getOwnPropertyNames(NativeRegExp)) {
    if (!["length", "name", "prototype", "escape"].includes(key)) delete NativeRegExp[key];
  }
  delete prototype.compile;
  Object.defineProperty(boundedRegExp, "prototype", {value: prototype});
  Object.defineProperty(prototype, "constructor", {value: boundedRegExp});
  if (NativeRegExp.escape) Object.defineProperty(boundedRegExp, "escape", {value: NativeRegExp.escape});
  Object.defineProperty(boundedRegExp, Symbol.species, {get() { return this; }});
  Object.freeze(prototype);
  Object.freeze(NativeRegExp);
  Object.freeze(boundedRegExp);
  Object.defineProperty(globalThis, "RegExp", {value: boundedRegExp, writable: false, configurable: false});
  Object.defineProperty(Math, "random", {value: undefined, writable: false, configurable: false});
  Object.freeze(Math);
  const freeze = value => {
    for (const key of Object.getOwnPropertyNames(value)) {
      const d = Object.getOwnPropertyDescriptor(value, key);
      if (d && d.value && (typeof d.value === "object" || typeof d.value === "function") && !Object.isFrozen(d.value)) freeze(d.value);
    }
    Object.freeze(value);
  };
  freeze(macroscope);
  Object.defineProperty(globalThis, "macroscope", {value: macroscope, writable: false, configurable: false});
})();
