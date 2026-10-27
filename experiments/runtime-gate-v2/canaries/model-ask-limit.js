function run() {
  for (let i=0;i<4;i++) {
    try { macroscope.model.ask("request " + i, {}, {type:"string"}); } catch (_) {}
  }
}
