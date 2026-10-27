function run() {
  /(capture)/.test("capture");
  for (const key of ["$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9", "input", "$_", "lastMatch", "$&", "lastParen", "$+", "leftContext", "$`", "rightContext", "$'"]) {
    if (key in RegExp) throw new Error("legacy static remains");
  }
  if ("compile" in RegExp.prototype) throw new Error("compile remains");
  if (!Object.isFrozen(RegExp) || !Object.isFrozen(RegExp.prototype)) throw new Error("not frozen");
  macroscope.log("DENIED: regex legacy statics and compile absent");
}
