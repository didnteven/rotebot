"use strict";
(() => {
  // Constructors remain reachable through prototype chains unless these links
  // are removed too (including the async and generator function families).
  for (const f of [function(){}, async function(){}, function*(){}, async function*(){}]) {
    const p = Object.getPrototypeOf(f);
    Object.defineProperty(p, "constructor", {value: undefined, writable: false, configurable: false});
    Object.freeze(p);
  }
  for (const name of ["eval", "Function", "AsyncFunction", "GeneratorFunction",
      "AsyncGeneratorFunction", "Worker", "SharedArrayBuffer", "Atomics", "Promise"]) {
    Object.defineProperty(globalThis, name, {value: undefined, writable: false, configurable: false});
  }
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
